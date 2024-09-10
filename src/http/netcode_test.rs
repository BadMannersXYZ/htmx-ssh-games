use std::{
    mem,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use axum::{
    extract::{Path, State},
    routing::{delete, get, put},
    Router,
};
use bitvec::{order::Lsb0, slice::BitSlice};
use hyper::{HeaderMap, StatusCode};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use tokio::{
    task::JoinHandle,
    time::{sleep, Instant},
};
use tracing::{debug, info, warn};

/* Router definition */

struct PingPong {
    ping: (Instant, Some(Instant)),
    pong: (Instant, Some(Instant)),
}

#[derive(Clone)]
struct AppState {
    nonogram: Arc<Mutex<HashMap<String, PingPong>>>,
}

/// A lazily-created Router, to be used by the SSH client tunnels.
pub async fn get_router() -> Router {
    let state = AppState {
        nonogram: Arc::new(Mutex::new(HashMap::new())),
    };
    Router::new()
        .route("/", get(index))
        .route("/ping", put(ping))
        .route("/ping2/:id", put(ping2))
        .with_state(state)
}

/* Main page elements */

fn style() -> &'static str {
    r#"
"#
}

fn script() -> &'static str {
    r#"
document.oncontextmenu = (e) => {
    if (e.target.closest('.checkbox')) {
        e.preventDefault();
    }
};
let baseTimestamp = document.timeline.currentTime;
let nonogramTimeLeft = null;
document.addEventListener('someEvent', (e) => {
    baseTimestamp = document.timeline.currentTime;
    nonogramTimeLeft = e.detail.value;
});
function updateFrame(currentTimestamp) {
    if (Number.isInteger(nonogramTimeLeft)) {
        let timerElapsed = document.getElementById("timer-elapsed");
        let timerDone = document.getElementById("timer-done");
        let timeLeft = nonogramTimeLeft + baseTimestamp - currentTimestamp;
        if (timeLeft <= 0) {
            if (timerElapsed) {
                timerElapsed.classList.add("hidden");
            }
            if (timerDone) {
                done.classList.remove("hidden");
            }
        } else {
            if (timerElapsed) {
                let minutes = Math.floor(timeLeft / 60000);
                let seconds = Math.floor((timeLeft % 60000) / 1000);
                timerElapsed.innerText = "Time left: " + minutes + ":" + (seconds < 10 ? "0" : "") + seconds;
                timerElapsed.classList.remove("hidden");
            }
            if (timerDone) {
                timerDone.classList.add("hidden");
            }
        }
    }
    requestAnimationFrame(updateFrame);
}
requestAnimationFrame(updateFrame);
"#
}

fn head() -> Markup {
    html! {
        (DOCTYPE)
        head {
            meta charset="utf-8";
            title { "Netcode test" }
            script src="https://unpkg.com/htmx.org@2.0.2" integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ" crossorigin="anonymous" {}
            style { (PreEscaped(style())) }
            script { (PreEscaped(script())) }
        }
    }
}

async fn index() -> Markup {
    html! {
        (head())
        body {
            h1 { "Netcode test" }
            main {
                p {}
            }
        }
    }
}

/* HTMX components */

fn timer(puzzle_state: NonogramState, time_left: Duration) -> Markup {
    if let NonogramState::Solved(success) = puzzle_state {
        let secs = success.as_secs();
        return html! {
            p #timer {
                "Solved in " (format!("{}:{:02}", secs / 60, secs % 60)) "!"
            }
        };
    };
    let secs = time_left.as_secs();
    html! {
        p #timer {
            span #timer-elapsed .hidden[time_left == Duration::ZERO] {
                "Time left: " (format!("{}:{:02}", secs / 60, secs % 60))
            }
            span #timer-done .hidden[time_left > Duration::ZERO] {
                "Time's up!"
            }
        }
    }
}
