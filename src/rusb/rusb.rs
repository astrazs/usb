use chrono::{
    prelude::{DateTime, Local},
    Duration,
};
// use std::time::{Duration as StdDuration , Instant};
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
use std::ffi::c_void;
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
            WS_OVERLAPPEDWINDOW,SetWindowLongPtrW, GetWindowLongPtrW, GWLP_USERDATA,
        },
    },
};
use std::sync::mpsc::{channel, Sender,Receiver};
const _STYLE: &str = manganis::mg!(file("assets/tailwind.css"));
const CLASS_NAME: PCWSTR = w!("USB_EVENT_WINDOW");
// static mut LAST_REFRESH: Option<Instant> = None;


async fn monitor_usb_changes() -> Result<()> {
    let (tx, rx) = channel::<()>();
    let tx_ptr: Box<Sender<()>> = Box::new(tx);
    // 创建一个线程用于处理 Windows 消息
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
                let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Sender<()>;
                // 将指针转换回 Box<Sender>
                let tx = Box::from_raw(tx_ptr);
                tx.send(()).unwrap(); // 发送一个空元组
            },
            DBT_DEVICEREMOVECOMPLETE => {
                println!("USB device removed!");
                let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Sender<()>;
                // 将指针转换回 Box<Sender>
                let tx = Box::from_raw(tx_ptr);
                tx.send(()).unwrap(); // 发送一个空元组
            },
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

// 刷新 
// fn trigger_refresh() {
//     if let Some(last_refresh) = unsafe { LAST_REFRESH } {
//         if last_refresh.elapsed() < StdDuration::from_secs(2) {
//             return; 
//         }
//     }
//     // 更新刷新时间
//     unsafe { LAST_REFRESH = Some(Instant::now()) };
    
// }


fn main() { 
    let config = dioxus::desktop::Config::new()
        .with_custom_head(format!(r#"<link rel="stylesheet" href="dist/{}">"#, _STYLE).to_string())
        .with_window(WindowBuilder::new().with_maximized(true));
    LaunchBuilder::desktop().with_cfg(config).launch(App);

}

#[component]
pub fn App() -> Element {
    // 启动一个任务来监听 USB 设备的变化
    tokio::spawn(async move {
        if let Err(e) = monitor_usb_changes().await {
            eprintln!("Error in usb_dirs: {}", e);
        }
    });
    rsx! {
        ImageGroupList{}
    }
    
}

//上传
async fn upload_folder_to_minio(local_upload_path: &str,object_name: &str,station_name: &str,canvas_name: &str,) -> core::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    let exists: bool = client
        .bucket_exists(&BucketExistsArgs::new(&bucket_name).unwrap())
        .await
        .unwrap();
    if !exists {
        client
            .make_bucket(&MakeBucketArgs::new(&bucket_name).unwrap())
            .await
            .unwrap();
    }
    let upload_name = format!("{}/{}", canvas_name, object_name);
    client
        .upload_object(
            &mut UploadObjectArgs::new(&bucket_name, &upload_name, &local_upload_path).unwrap(),
        )
        .await
        .unwrap();
    println!("OK");
    Ok(())
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

#[derive(PartialEq, Clone)]
struct Groups {
    images: Vec<(String, DateTime<Local>)>,
    selected: bool,
}
// 3.将图片按时间分组
async fn group_images_upon_time() -> Vec<Groups> {
    let max_interval = Duration::minutes(30);
    let mut image_messsage = get_images_from_usb();
    image_messsage.sort_by_key(|f| f.1);
    let new_groups = image_messsage
        .windows(2)
        .enumerate()
        .filter(move |(_, w)| w[1].1 - w[0].1 > max_interval)
        .map(|(i, _)| i + 1)
        .chain(std::iter::once(image_messsage.len()))
        .scan(0, |start, end| {
            let group = &image_messsage[*start..end];
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
// 4.将勾选的图片进行上传（上传的逻辑在外面）
// 获取勾选的图片 -->要从rsx中获得，就是要checked
// fn get_select_imagegrops(groups:&Signal<Vec<Groups>>){

// }
// 将勾选的图片上传
async fn select_imagegroups_upload(groups: &Signal<Vec<Groups>>,station_name: &Signal<String>,canvas_name: &Signal<String>,) {
    let selected_image_paths: Vec<String> = groups
        .read()
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

// 自定义输入框组件---input
#[component]
fn InputBox(label: String, name_value: Signal<String>) -> Element {
    rsx! {
        div{
            span{
                class:"span-gray",
                "{label}: "
            }
            input{
                class:"input-text",r#type: "text",name:"{label}",
                oninput:move |e| name_value.set(e.value()),
            }
        }
    }
}

// 自定义展示图片列表部分
#[component]
fn ImageGroup(group: Groups, index: usize, select_group: EventHandler<bool>) -> Element {
    let selected = group.selected;
    rsx! {
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
                // class: "m-2 grid gap-3 grid-cols-8",
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
static GROUPS : GlobalSignal<Groups> = Signal::global(|| Vec::new());
#[component]
fn ImageGroupList() -> Element {
    // let rx = ImageGroupListProps.rx; // 获取 Receiver
    let mut groups = use_signal(Vec::<Groups>::new);
    // 在组件初始化时加载图片数据
    spawn(async move {
        let mut groups_write = groups.write();
        groups_write.clear();
        groups_write.extend(group_images_upon_time().await);
    });
    // 在这里监听通道
    // while let Ok(_) = rx.try_recv() {
    //     spawn(async move {
    //         let mut groups_write = groups.write();
    //         groups_write.clear();
    //         groups_write.extend(group_images_upon_time().await);
    //     });
    // };
    //checked选择
    let select_group = move |selected: bool| {
        let group_index = groups
            .read()
            .iter()
            .position(|g| g.selected)
            .unwrap_or_default();
        if let Some(g) = groups.write().get_mut(group_index) {
            g.selected = selected;
        }
    };
    // 上传按钮
    let station_name = use_signal(String::new);
    let canvas_name = use_signal(String::new);
    let on_upload_click = move |_| {
        spawn(async move {
            select_imagegroups_upload(&groups, &station_name, &canvas_name).await;
        });
    };
    rsx! {
        div {
            class: "m-2 border border-gray-300",
            div{
                class:"ml-2 grid gap-6 mb-6 md:grid-cols-2 mt-4 mr-2",
                InputBox{label:"站点名".to_string(),name_value:station_name}
                InputBox{label:"相机名".to_string(),name_value:canvas_name}
                div{
                    button{
                        class:"btn-blue ml-2 centre",
                        onclick: on_upload_click,
                        "上传",
                    }
                }
            }
            div {
                class: "m-2",
                for (group_index, group) in groups.read().iter().enumerate() {
                    ImageGroup{ group: group.clone(), index: group_index, select_group: select_group.clone() }
                }
            }
        }
    }
}
