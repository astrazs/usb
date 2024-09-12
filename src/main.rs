use chrono::{prelude::{DateTime, Local},Duration,};
use dioxus::prelude::*;
use dioxus_desktop::WindowBuilder;
use futures::executor::block_on;
use glob::glob;
use minio::s3::{
    args::{BucketExistsArgs, MakeBucketArgs, UploadObjectArgs},
    client::Client,
    creds::StaticProvider,
    http::BaseUrl,
};
use sysinfo::Disks;
use tokio::sync::mpsc::{channel, Sender};
use windows::{
    core::*,
    Win32::{
        Devices::Usb::GUID_DEVINTERFACE_USB_DEVICE,
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
            RegisterDeviceNotificationW, UnregisterDeviceNotification, DBT_DEVICEARRIVAL,
            DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE,
            DEV_BROADCAST_DEVICEINTERFACE_W, WINDOW_EX_STYLE, WM_DEVICECHANGE, WNDCLASSW,
            WS_OVERLAPPEDWINDOW,SetWindowLongPtrW, GetWindowLongPtrW, GWLP_USERDATA,
        },
    },
};
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
const _STYLE: &str = manganis::mg!(file("assets/tailwind.css"));
const MAX_INTERVAL: Duration = Duration::minutes(30);
const CLASS_NAME: PCWSTR = w!("USB_EVENT_WINDOW");
static ARRIVAL_COUNT: AtomicUsize = AtomicUsize::new(0);
// static REMOVAL_COUNT: AtomicUsize = AtomicUsize::new(0);

async fn monitor_usb_changes(tx:Sender<u32>) -> Result<()> {
    // let tx_ptr: Box<_> = Box::new(Arc::new(tx));
    let tx_arc = Arc::new(tx);
    tokio::task::spawn_blocking(move || unsafe {
        // 注册窗口类 
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: GetModuleHandleW(None).unwrap().into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&wc);

        // 创建隐藏窗口
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),  
            CLASS_NAME,
            w!("USB Event Window"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        )
        .unwrap();
        // SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(tx_ptr) as isize);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&tx_arc as *const _) as isize);

        // 创建设备通知
        let dbdi = DEV_BROADCAST_DEVICEINTERFACE_W {
            dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
            dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE.0,
            dbcc_classguid: GUID_DEVINTERFACE_USB_DEVICE,
            ..Default::default()
        };

        let h_notification = RegisterDeviceNotificationW(
            hwnd,
            &dbdi as *const _ as *const c_void,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
        .unwrap();

        // 消息循环
        let mut msg = Default::default();
        while GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0).into() {
            DispatchMessageW(&msg);
        }

        // 清理资源
        UnregisterDeviceNotification(h_notification).unwrap();

        // 清除 GWLP_USERDATA 中的数据
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    })
    .await
    .unwrap();

    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_DEVICECHANGE {
        match wparam.0 as u32 {
            DBT_DEVICEARRIVAL => {
                let count = ARRIVAL_COUNT.fetch_add(1, Ordering::Relaxed);
                println!("{count}");
                if count % 2 == 1 {
                    println!("USB device arrived1!");
                    // let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Arc<mpsc::Sender<u32>>;
                    let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Arc<Sender<u32>>;
                    println!("USB device arrived2!");
                    // let tx = Box::from_raw(tx_ptr);
                    let tx = &*tx_ptr;
                    println!("USB device arrived3!");
                    tx.blocking_send(wparam.0 as u32).unwrap();
                    println!("USB device arrived4!");
                }
            },
            DBT_DEVICEREMOVECOMPLETE => {
                println!("USB device removed!");
                
            },
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[derive(Clone, Debug , PartialEq)]
pub struct Groups {
    images: Vec<(String, DateTime<Local>)>,
    selected: bool,
}

#[derive(Clone)]
pub struct ImageManager {  
    pub groups: Vec<Groups>,
}

impl ImageManager {
    pub fn new() -> Self {
        let images = get_images_from_usb(); 
        let groups = block_on(group_images_upon_time(images)); 

        Self {
            groups,
        }
    }

    // 照片勾选
    pub fn toggle_group_selection(&mut self, index: usize) {
        if let Some(group) = self.groups.get_mut(index) {
            group.selected = !group.selected;
        }
    }

    pub async fn update_on_usb_arrival(&mut self) {
        let images = get_images_from_usb(); 
        let groups = block_on(group_images_upon_time(images)); 
        self.groups = groups;
    }

    pub async fn select_imagegroups_upload(&self,station_name: &Signal<String>,canvas_name: &Signal<String>) {
        let selected_image_paths: Vec<String> = self
            .groups
            .iter()
            .filter(|group| group.selected)
            .flat_map(|group| {
                group
                    .images
                    .iter()
                    .map(|(path, _)| path.replace("\\", "/").clone())
            })
            .collect();
        for images_path in selected_image_paths.iter() {
            let file_name = images_path.split('/').last().unwrap_or(&"").to_string();
            let future = async move {
                let station_clone = station_name.to_string();
                let canvas_clone = canvas_name.to_string();
                upload_folder_to_minio(&images_path, &file_name, &station_clone, &canvas_clone)
                    .await
                    .unwrap();
            };
            let _ = block_on(future);
        }
    }

}

// 1.获取移动硬盘USb的地址
fn get_usb_address() -> Vec<String> {
    let disks = Disks::new_with_refreshed_list();
    let dirs: Vec<String> = disks
        .iter()
        .filter_map(|disk| {
            disk.is_removable()
                .then(|| disk.mount_point().to_str().unwrap().to_string())
        })
        .collect();
    dirs
}

// 2.遍历地址获得地址中所有图片
fn get_images_from_usb() -> Vec<(String, DateTime<Local>)> {
    let dirs = get_usb_address();
    let filenames: Vec<(String, DateTime<Local>)> = dirs
        .iter()
        .flat_map(|dir| {
            glob(&format!("{}/**/*.JPG", dir))
                .into_iter()
                .flat_map(|paths| {
                    paths.into_iter().filter_map(|path| {
                        let file = path.ok()?;
                        Some((
                            file.to_str().unwrap_or_default().to_string(),
                            file.metadata().ok()?.modified().ok()?.into(),
                        ))
                    })
                })
        })
        .collect();
    filenames
}

// 3.将图片按时间分组
async fn group_images_upon_time(mut images: Vec<(String, DateTime<Local>)>) -> Vec<Groups> {
    images.sort_by_key(|f| f.1);
    let new_groups = images
        .windows(2)
        .enumerate()
        .filter(move |(_, w)| w[1].1 - w[0].1 > MAX_INTERVAL)
        .map(|(i, _)| i + 1)
        .chain(std::iter::once(images.len()))
        .scan(0, |start, end| {
            let group = &images[*start..end];
            *start = end; 
            Some(group)
        })
        .map(|group| Groups {
            images: group.iter().map(|g| g.clone()).collect(),
            selected: false,
        })
        .collect::<Vec<_>>();
    new_groups
}

async fn upload_folder_to_minio(local_upload_path: &str,object_name: &str,station_name: &str,camera_name: &str,) -> core::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let base_url = "http://10.230.30.210".parse::<BaseUrl>()?;
    let static_provider = StaticProvider::new(
        "RU6W8QulqzQTa91pMllu",
        "PyUph1oeZHnuaapB98Td5ttzQnPXwbch7hN78AEz",
        None,
    );
    let client = Client::new(
        base_url.clone(),
        Some(Box::new(static_provider)),
        None,
        None,
    )
    .unwrap();
    let bucket_name = station_name;
    let exists = client
        .bucket_exists(&BucketExistsArgs::new(&bucket_name).unwrap())
        .await
        .unwrap();
    if !exists {
        client
            .make_bucket(&MakeBucketArgs::new(&bucket_name).unwrap())
            .await
            .unwrap();
    }
    let upload_name = format!("{}/{}", camera_name, object_name);
    client
        .upload_object(
            &mut UploadObjectArgs::new(&bucket_name, &upload_name, &local_upload_path).unwrap(),
        )
        .await
        .unwrap();
    println!("OK");
    Ok(())
}

#[component]
fn ImageGroup(group: Groups, index: usize, select_group: EventHandler<bool>) -> Element {
    let selected = group.selected;
    rsx! {
        if !group.images.is_empty(){
            div {
                class: "border border-gray ml-1 mb-4 shadow-md rounded-lg",
                div {
                    class: "flex m-2",
                    div {
                        class: "flex items-center h-5",
                        input {
                            class: "ckb-blue",
                            r#type: "checkbox",
                            checked: selected,
                            oninput: move |evt| {
                                select_group.call(evt.checked());
                            },
                        }
                    }
                    div {
                        class: "ms-2 text-sm",
                        span {
                            class: "ml-2 ckb-span-ul",
                            "当前组数为：{index + 1}"
                        }
                        span {
                            class: "ml-8 ckb-span-ul",
                            "该组图片总数：{group.images.len()}"
                        }
                        br {}
                        span {
                            class: "ckb-span-li",
                            "拍摄时间：",
                            {format!("{}", group.images.first().unwrap().1.format("%Y年%m月%d日 %H时%M分"))},
                            "-",
                            {format!("{}", group.images.last().unwrap().1.format("%H时%M分"))},
                        }
                    }
                }
                div{
                    if group.images.len() < 15{
                        div{
                            class:"m-2 grid gap-3 grid-cols-8",
                            for (index,(filename, _timestamp)) in group.images.iter().enumerate() {
                                if index < 7 { 
                                    img {
                                        src: format!("./{}", filename),
                                        width: "170px",class:"m-2 rounded",
                                    }
                                }
                                
                            }
                        }
                    }
                    else {
                        div {
                            class:"m-2 grid gap-3 grid-cols-8",
                            for (index,(filename, _timestamp)) in group.images.iter().enumerate() {
                                if index < 7 { 
                                    img {
                                        src: format!("./{}", filename),
                                        width: "170px",
                                        class:"m-2 rounded",
                                    }
                                }
                                
                            }
                            "......"
                        }
                        div {
                            class:"m-2 grid gap-3 grid-cols-8",
                            "......"
                            for (index,(filename, _timestamp)) in group.images.iter().enumerate() {
                                if  index >= group.images.len()-7{
                                    img {
                                        src: format!("./{}", filename),
                                        width: "170px",
                                        class:"m-2 rounded",
                                    }
                                }
                            }
                        }
                    }
                }
            }           
        }
    }
}


#[component]
fn ImageGroupList(manager: Signal<ImageManager>) -> Element {
    let mut station_name = use_signal(String::new);
    let mut canvas_name = use_signal(String::new);
    rsx! {
        div {
            class: "m-2 border border-gray-300",
            div{
                class:"ml-2 grid gap-6 mb-6 md:grid-cols-2 mt-4 mr-2",
                div{
                    span{
                        class:"span-gray",
                        "站点名："
                    }
                    input{
                        class:"input-text",r#type: "text",
                        oninput:  move  |e| {
                            station_name.set(e.value());
                        },
                    }
                }
                div{
                    span{
                        class:"span-gray",
                        "相机名："
                    }
                    input{
                        class:"input-text",r#type: "text",
                        oninput:  move  |e| {
                            canvas_name.set(e.value());
                        },
                    }
                }
                div{
                    button{
                        class:"btn-blue ml-2 centre",
                        onclick:  move  |_| {
                            async move{
                                manager.read().select_imagegroups_upload(&station_name, &canvas_name).await;
                            }
                           
                        },
                        "上传",
                    }
                }
            }
            div {
                class: "m-2",
                for (group_index, group) in manager.read().groups.iter().enumerate() {
                    ImageGroup {
                        group: group.clone(),
                        index: group_index,
                        select_group: move |_| manager.write().toggle_group_selection(group_index),
                    }
                }
            }
        }
    }
}
// static MANAGER:GlobalSignal<ImageManager> = Signal::global(||ImageManager::new());
#[component]
fn App() -> Element {
    let mut manager = use_signal(|| ImageManager::new());
    use_effect(move ||{
        let (tx, mut rx) = channel(32);
        spawn(async move {
            print!("1");
            if let Err(e) = monitor_usb_changes(tx).await {
                eprintln!("Error in monitor_usb_changes: {}", e);
            }
        });
        spawn(async move {
            println!("USB device arrivedw!");
            while let Some(_) = rx.recv().await { 
                println!("USB device arrivedwwwwww!");
                manager.write().update_on_usb_arrival().await; 
                println!("USB device arrivedwww!");
            }
        });
    });

    rsx!{
        ImageGroupList{ manager : manager}
    }
}

fn main() {
    let config = dioxus::desktop::Config::new()
        .with_custom_head(format!(r#"<link rel="stylesheet" href="dist/{}">"#, _STYLE).to_string())
        .with_window(WindowBuilder::new().with_maximized(true).with_title("usb"));
    LaunchBuilder::desktop().with_cfg(config).launch(App);
}
