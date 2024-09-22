use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    routing::{delete, get, put},
    Router,
};
use bitvec::{array::BitArray, order::Lsb0, BitArr};
use hyper::StatusCode;
use maud::{html, Markup, DOCTYPE};

#[derive(Clone)]
struct AppState {
    checkboxes: Arc<Mutex<BitArr!(for CHECKBOX_WIDTH*CHECKBOX_HEIGHT, in usize, Lsb0)>>,
}

const CHECKBOX_WIDTH: usize = 20;
const CHECKBOX_HEIGHT: usize = 20;

/// A lazily-created Router, to be used by the SSH client tunnels.
pub fn get_router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/checkboxes", get(all_checkboxes))
        .route("/checkbox/:id", put(mark_checkbox))
        .route("/checkbox/:id", delete(unmark_checkbox))
        .with_state(AppState {
            checkboxes: Arc::new(Mutex::new(BitArray::ZERO)),
        })
}

fn style() -> &'static str {
    r#"
body {
    width: fit-content;
}
ul {
    display: grid;
    list-style: none;
    padding-left: 0;
    gap: 2px;
}
li {
    width: 20px;
    height: 20px;
}
"#
}

fn head() -> Markup {
    html! {
        (DOCTYPE)
        head {
            meta charset="utf-8";
            title { (CHECKBOX_WIDTH*CHECKBOX_HEIGHT) " Checkboxes" }
            script src="https://unpkg.com/htmx.org@2.0.2" integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ" crossorigin="anonymous" {}
            style { (style()) }
        }
    }
}

async fn index() -> Markup {
    html! {
        (head())
        body {
            h1 { (CHECKBOX_WIDTH*CHECKBOX_HEIGHT) " Checkboxes" }
            div hx-get="/checkboxes" hx-trigger="load" hx-swap="outerHTML" {}
        }
    }
}

async fn all_checkboxes(State(state): State<AppState>) -> Markup {
    html! {
        ul hx-get="/checkboxes" hx-trigger="every 3s" style=(format!("grid-template-columns: repeat({}, minmax(0, 1fr));", CHECKBOX_WIDTH)) hx-swap="outerHTML" {
            @for (id, checkbox) in state.checkboxes.lock().unwrap()[..CHECKBOX_WIDTH*CHECKBOX_HEIGHT].iter().by_vals().enumerate() {
                li {
                    @if checkbox {
                        (checked(id))
                    } @else {
                        (unchecked(id))
                    }
                }
            }
        }
    }
}

fn checked(id: usize) -> Markup {
    html! {
        input id=(format!("cb-{}", id)) type="checkbox" hx-delete=(format!("/checkbox/{}", id)) hx-trigger="click" checked {}
    }
}

fn unchecked(id: usize) -> Markup {
    html! {
        input id=(format!("cb-{}", id)) type="checkbox" hx-put=(format!("/checkbox/{}", id)) hx-trigger="click" {}
    }
}

async fn mark_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> Result<Markup, StatusCode> {
    match state.checkboxes.lock().unwrap().get_mut(id) {
        None => Err(StatusCode::NOT_FOUND),
        Some(mut checkbox) => {
            *checkbox = true;
            Ok(checked(id))
        }
    }
}

async fn unmark_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> Result<Markup, StatusCode> {
    match state.checkboxes.lock().unwrap().get_mut(id) {
        None => Err(StatusCode::NOT_FOUND),
        Some(mut checkbox) => {
            *checkbox = false;
            Ok(unchecked(id))
        }
    }
}
