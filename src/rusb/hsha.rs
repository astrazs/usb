use chrono::{
    prelude::{DateTime, Local},
    Duration,
};
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
            WS_OVERLAPPEDWINDOW,SetWindowLongPtrW, GetWindowLongPtrW, GWLP_USERDATA
        },
    },
};
use std::{ffi::c_void, ptr::null, sync::mpsc::Receiver};
use std::sync::mpsc::{channel, Sender};
const _STYLE: &str = manganis::mg!(file("assets/tailwind.css"));
const MAX_INTERVAL: Duration = Duration::minutes(30);
const CLASS_NAME: PCWSTR = w!("USB_EVENT_WINDOW");

async fn monitor_usb_changes(tx:Sender<u32>) -> Result<()> {
    let tx_ptr: Box<_> = Box::new(tx);
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
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(tx_ptr) as isize);
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
    })
    .await
    .unwrap();

    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_DEVICECHANGE {
        match wparam.0 as u32 {
            DBT_DEVICEARRIVAL => {
                println!("USB device arrived!");
                let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Sender<u32>;
                // 将指针转换回 Box<Sender>
                let tx = Box::from_raw(tx_ptr);
                tx.send(wparam.0 as u32).unwrap();
            },
            DBT_DEVICEREMOVECOMPLETE => {
                println!("USB device removed!");
                let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Sender<u32>;
                // 将指针转换回 Box<Sender>
                let tx = Box::from_raw(tx_ptr);
                tx.send(wparam.0 as u32).unwrap();
            },
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}
#[derive(Clone, Debug, PartialEq)]
pub struct Groups {
    images: Vec<(String, DateTime<Local>)>,
    selected: bool,
}
#[derive(Clone)]
pub struct ImageManager {
    pub groups: Vec<Groups>,
    pub station_name: String,
    pub camera_name: String,
}
impl ImageManager {
    pub fn new(station_name: String, camera_name: String) -> Self {
        let images = get_images_from_usb(); //遍历地址获取所有图片
        let groups = block_on(group_images_upon_time(images)); //将图片按时间分组

        Self {
            groups,
            station_name,
            camera_name,
        }
    }

    // 照片勾选
    pub fn toggle_group_selection(&mut self, index: usize) {
        if let Some(group) = self.groups.get_mut(index) {
            group.selected = !group.selected;
        }
    }

    fn set_station_name(&mut self, station_name: String) {
        self.station_name = station_name;
    }

    fn set_camera_name(&mut self, camera_name: String) {
        self.camera_name = camera_name;
    }

    pub fn update_on_usb_arrival(&mut self) {
        self.groups.clear();
        let images = get_images_from_usb(); // 遍历地址获取所有图片
        let groups = block_on(group_images_upon_time(images)); // 将图片按时间分组【
        self.groups = groups;
        // Self{
        //     groups,
        //     station_name:self.camera_name.clone(),
        //     camera_name:self.camera_name.clone(),
        // }
    }

    // 将勾选的图片上传
    pub fn upload_selected(&self) {
        let select_image_paths: Vec<String> = self
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

        for images_path in select_image_paths.into_iter() {
            let file_name = images_path.split('/').last().unwrap_or(&"").to_string();
            let future = async move {
                let station_name = self.station_name.clone();
                let camera_name = self.camera_name.clone();
                upload_folder_to_minio(&images_path, &file_name, &station_name, &camera_name)
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

//上传
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
                div {
                    div {
                        class:"m-2 grid gap-3 grid-cols-8",
                        for (index,(filename, _timestamp)) in group.images.iter().enumerate() {
                            if index < 7 { 
                                img {
                                    src: format!("./{}", filename),
                                    width: "170px",class:"m-2 rounded",
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
                                    width: "170px",class:"m-2 rounded",
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
                            manager.write().set_station_name(e.value());
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
                            manager.write().set_camera_name(e.value());
                        },
                    }
                }
                div{
                    button{
                        class:"btn-blue ml-2 centre",
                        onclick:  move  |_| {
                            manager.read().upload_selected();
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

#[component]
fn App() -> Element {
    let (tx, rx) = channel();
    let mut  manager = use_signal(|| ImageManager::new(String::from("StationName"), String::from("CameraName")));
    use_effect(move ||{
        let tx =tx.clone();
        spawn(async move {
            if let Err(e) = monitor_usb_changes(tx).await {
                eprintln!("Error in usb_dirs: {}", e);
            }
        });
    });

    // spawn(async move {
    //     if let Err(e) = monitor_usb_changes(tx).await {
    //         eprintln!("Error in usb_dirs: {}", e);
    //     }
    // });
    // spawn(async move {
    //     while let Ok(_) = rx.recv(){
    //         println!("USB device arrivedwwwwww!");
    //         manager.write().update_on_usb_arrival();
    //         println!("USB device arrivedwww!");
    //     }
    // });

    use_effect(move ||{
        while let Ok(_) = rx.recv(){
            println!("USB device arrivedwwwwww!");
            manager.write().update_on_usb_arrival();
            println!("USB device arrivedwww!");
        }
    });
    rsx!{
        ImageGroupList{
            manager : manager
        }
    }

}

fn main() {
    let config = dioxus::desktop::Config::new()
        .with_custom_head(format!(r#"<link rel="stylesheet" href="dist/{}">"#, _STYLE).to_string())
        .with_window(WindowBuilder::new().with_maximized(true));
    LaunchBuilder::desktop().with_cfg(config).launch(App);
  
}
