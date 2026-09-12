use dioxus::prelude::*;
use dioxus_qonversion as _;

fn main() {
    launch(app);
}

fn app() -> Element {
    rsx!(
        div {
            display: "flex",
            justify_content: "center",
            h3 {
                "dioxus-qonversion — unofficial Qonversion bridge for Dioxus mobile"
            }
        }
    )
}
