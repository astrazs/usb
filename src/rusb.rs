use dioxus::prelude::*;
const _STYLE: &str = manganis::mg!(file("assets/tailwind.css"));

fn main() {
    let config = dioxus::desktop::Config::new()
        .with_custom_head(format!(r#"<link rel="stylesheet" href="dist/{}">"#, _STYLE).to_string());
    LaunchBuilder::desktop().with_cfg(config).launch(App);
    
}
#[component]
pub fn App() -> Element {
    rsx! {
        watch_usb{}
    }
}

#[component]
fn watch_usb() -> Element {
    
}