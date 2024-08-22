use chrono::{prelude::{DateTime, Local},Duration,};
use dioxus::prelude::*;
use futures::executor::block_on;
use glob::glob;
use minio::s3::{
    args::{BucketExistsArgs, MakeBucketArgs, UploadObjectArgs},
    client::Client,
    creds::StaticProvider,
    http::BaseUrl,
};
use sysinfo::{Disks, System};
const _STYLE: &str = manganis::mg!(file("assets/tailwind.css"));

fn main() {
    let config = dioxus::desktop::Config::new()
        .with_custom_head(format!(r#"<link rel="stylesheet" href="dist/{}">"#, _STYLE).to_string());
    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

#[component]
pub fn App() -> Element {
    rsx! {
        upload_image_to_files{}
    }
}

async fn upload_folder_to_minio(local_upload_path: &str,object_name: &str,station_name: &str,canvas_name: &str,) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

fn show_all_disks() {
    let mut sys = System::new_all();
    sys.refresh_all();
    println!("=> disks:");
    let disks = Disks::new_with_refreshed_list();
    for disk in disks.iter() {
        if disk.is_removable(){
            println!(" Mount point: {:?}",disk.mount_point());
        }
    }
}

#[derive(PartialEq, Clone)]
struct Groups {
    images: Vec<(String, DateTime<Local>)>,
    selected: bool,
}

#[component]
fn upload_image_to_files() -> Element {
    let _ = show_all_disks();
    let mut groups = use_signal(Vec::<Groups>::new);
    let on_image_selected = move |evt: dioxus_core::Event<FormData>| {
        if let Some(file_engine) = &evt.files() {
            let filenames: Vec<(String, DateTime<Local>)> = file_engine
                .files()
                .iter()
                .flat_map(|dir| {
                    glob(&format!("{}/**/*.JPG", dir))
                        .into_iter()
                        .flat_map(|paths| {
                            paths.into_iter().filter_map(|path| {
                                let file = path.ok()?;
                                Some((
                                    file.display().to_string(),
                                    file.metadata().ok()?.modified().ok()?.into(),
                                ))
                            })
                        })
                })
                .collect();

            let max_interval = Duration::minutes(30);
            let mut data = filenames.clone();
            data.sort_by_key(|f| f.1);
            let new_groups = data
                .windows(2)
                .enumerate()
                .filter(move |(_, w)| w[1].1 - w[0].1 > max_interval)
                .map(|(i, _)| i + 1)
                .chain(std::iter::once(data.len()))
                .scan(0, |start, end| {
                    let group = &data[*start..end];
                    *start = end;
                    Some(group)
                })
                .map(|group| Groups {
                    images: group.iter().map(|g| g.clone()).collect(),
                    selected: false,
                })
                .collect::<Vec<_>>();
            groups.write().extend(new_groups);
        }
    };

    let mut station_name = use_signal(String::new);
    let mut canvas_name = use_signal(String::new);

    let on_upload_click = move |_| {
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
                let station_clo = station_name.clone().to_string();
                let canvas_clo = canvas_name.clone().to_string();
                upload_folder_to_minio(&images_path, &file_name, &station_clo, &canvas_clo)
                    .await
                    .unwrap();
            };
            let _ = block_on(future);
        }
    };

    rsx! {
        div {
            class: "m-2 border border-gray-300",
            div {
                class:"m-2",
                input {
                    class:"w-76 block text-gray-900 border border-gray-300 rounded-lg cursor-pointer bg-gray-50 focus:outline-none text-m",
                    r#type: "file",directory: true,multiple: true,
                    onchange: on_image_selected,
                }
            }
            div{
                class:"m-2",
                div{
                    span{
                        class:"span-gray","站点名：",
                    }
                    input{
                        class:"input-text",r#type: "text",name:"station_name",placeholder:"梁溪变",
                        oninput: move |e| station_name.set(e.value()),
                    }
                }
                div{
                    class:"mt-2",
                    span{
                        class:"span-gray","相机名:",
                    }
                    input{
                        class:"input-text ",r#type: "text",name:"canvas_name",placeholder:"索尼",
                        oninput: move |e| canvas_name.set(e.value()),
                    }
                }
                div{
                    button{
                        class:"btn-blue",
                        onclick: on_upload_click,
                        "上传",
                    }
                }
            }
            div {
                class: "m-2",
                for (group_index, group) in groups.read().iter().enumerate() {
                    div {
                        class:"border border-gray ml-1 mb-4 shadow-md rounded-lg ",
                        div{
                             class:"flex m-2",
                            div {
                                class:"flex items-center h-5",
                                input {
                                    class:"ckb-blue",r#type: "checkbox",
                                    checked: group.selected,
                                    oninput: move |evt| {
                                        if let Some(g) = groups.write().get_mut(group_index) {
                                            g.selected = evt.checked();
                                        }
                                    },
                                }
                            }
                            div{
                                class:"ms-2 text-sm",
                                span{
                                    class:"ml-2 ckb-span-ul",
                                    "当前组数为：{group_index+1}",
                                }
                                span{
                                    class:"ml-8 ckb-span-ul",
                                    "该组图片总数：{group.images.len()}",
                                }
                                br{}
                                span{
                                    class:"ckb-span-li",
                                    "拍摄时间："
                                    {format!("{}", group.images.first().unwrap().1.format("%Y年%m月%d日 %H时%M分"))},
                                    "-"
                                    {format!("{}", group.images.last().unwrap().1.format("%H时%M分"))},
                                }
                            }
                        }
                        div {
                            class:"flex flex-wrap m-2",
                            for (filename, _timestamp) in group.images.iter().take(10) {
                                    img {
                                        src: format!("./{}", filename.replace("\\", "/")),
                                        width: "120px",class:"m-2 rounded",
                                    }
                            }
                        }
                    }
                }
            }
        }
    }
}
