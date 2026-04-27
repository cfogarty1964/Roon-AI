//! Navigation component using Tailwind CSS.

use crate::app::embedded_assets::LOGO_DATA_URL;
use crate::app::Route;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct NavProps {
    /// The currently active page ID (e.g., "zones", "settings")
    pub active: String,
}

/// Navigation bar using Tailwind CSS with mobile toggle.
#[component]
pub fn Nav(props: NavProps) -> Element {
    let mut menu_open = use_signal(|| false);

    let nav_link_class = |page: &str| {
        if props.active == page {
            "nav-link-active"
        } else {
            "nav-link"
        }
    };

    let mobile_menu_class = if menu_open() {
        "block lg:hidden"
    } else {
        "hidden lg:hidden"
    };

    rsx! {
        nav { class: "nav-container",
            div { class: "nav-inner",
                // Logo / Brand
                div { class: "flex items-center",
                    Link { class: "nav-brand flex items-center", to: Route::ConversationalAi {},
                        img {
                            src: "{*LOGO_DATA_URL}",
                            alt: "Roon AI",
                            class: "h-6 w-6 rounded"
                        }
                    }
                }

                // Desktop navigation - use Link for client-side routing (no page reload)
                div { class: "hidden lg:flex items-center space-x-4",
                    Link { class: nav_link_class("conversational"), to: Route::ConversationalAi {}, "Conversational AI" }
                    Link { class: nav_link_class("library"), to: Route::Library {}, "Library" }
                    Link { class: nav_link_class("settings"), to: Route::Settings {}, "Settings" }
                }

                // Mobile menu button
                div { class: "lg:hidden",
                    button {
                        class: "nav-mobile-toggle",
                        r#type: "button",
                        onclick: move |_| menu_open.toggle(),
                        span { class: "sr-only", "Toggle menu" }
                        if menu_open() {
                            // X icon
                            svg { class: "h-6 w-6", fill: "none", view_box: "0 0 24 24", stroke: "currentColor", "stroke-width": "2",
                                path { "stroke-linecap": "round", "stroke-linejoin": "round", d: "M6 18L18 6M6 6l12 12" }
                            }
                        } else {
                            // Hamburger icon
                            svg { class: "h-6 w-6", fill: "none", view_box: "0 0 24 24", stroke: "currentColor", "stroke-width": "2",
                                path { "stroke-linecap": "round", "stroke-linejoin": "round", d: "M4 6h16M4 12h16M4 18h16" }
                            }
                        }
                    }
                }
            }

            // Mobile menu - use Link for client-side routing
            div { class: "{mobile_menu_class}", id: "mobile-menu",
                div { class: "px-2 pt-2 pb-3 space-y-1",
                    Link { class: nav_link_class("conversational"), to: Route::ConversationalAi {}, onclick: move |_| menu_open.set(false), "Conversational AI" }
                    Link { class: nav_link_class("library"), to: Route::Library {}, onclick: move |_| menu_open.set(false), "Library" }
                    Link { class: nav_link_class("settings"), to: Route::Settings {}, onclick: move |_| menu_open.set(false), "Settings" }
                }
            }
        }
    }
}
