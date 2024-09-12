use std::{
    collections::HashMap,
    hash::Hash,
    mem,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use anyhow::Result;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Form, Router,
};
use bitvec::{order::Lsb0, slice::BitSlice};
use hyper::{HeaderMap, StatusCode};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use rand::{seq::SliceRandom, thread_rng, Rng};
use random_color::{Luminosity, RandomColor};
use serde::Deserialize;
use tokio::{
    sync::watch::{self, Receiver, Sender},
    task::JoinHandle,
    time::{sleep, Instant},
};
use tracing::{debug, warn};

use crate::nonogram::nonogrammed::{get_puzzle_data, NonogrammedPuzzle, NONOGRAMMED_PUZZLE_LIST};

/* Type defintions */

#[derive(Copy, Clone, PartialEq)]
enum NonogramState {
    Unsolved,
    Solved(Duration),
    Failed,
}

struct Timer {
    start: Instant,
    duration: Duration,
    join_handle: Option<JoinHandle<()>>,
}

#[derive(Copy, Clone, PartialEq)]
enum CheckboxState {
    Empty,
    Flagged,
    Marked,
}

struct Nonogram {
    puzzle_list: Vec<u32>,
    state: NonogramState,
    puzzle_sender: Sender<NonogrammedPuzzle>,
    checkboxes: Vec<CheckboxState>,
    timer: Timer,
}

#[derive(PartialEq, Copy, Clone)]
struct CursorPosition(i32, i32);

#[derive(PartialEq, PartialOrd, Eq, Hash, Copy, Clone)]
struct CursorId(u64);

struct Cursor {
    id: CursorId,
    modified_at: Instant,
    position: CursorPosition,
    color: [u8; 3],
}

impl Cursor {
    fn new(id: CursorId, position: CursorPosition) -> Self {
        let color = RandomColor::new()
            .luminosity(Luminosity::Light)
            .seed(id.0)
            .to_rgb_array();
        Cursor {
            id,
            modified_at: Instant::now(),
            position,
            color,
        }
    }
}

#[derive(Deserialize, Debug)]
struct CursorsPayload {
    id: u64,
    #[serde(rename = "mouseX")]
    mouse_x: i32,
    #[serde(rename = "mouseY")]
    mouse_y: i32,
}

static VERSION: LazyLock<u32> = LazyLock::new(|| {
    let mut rng = rand::thread_rng();
    rng.gen()
});

/* Router definition */

#[derive(Clone)]
struct AppState {
    nonogram: Arc<Mutex<Nonogram>>,
    puzzle: Arc<Receiver<NonogrammedPuzzle>>,
    cursors: Arc<Mutex<HashMap<CursorId, Cursor>>>,
}

/// A lazily-created Router, to be used by the SSH client tunnels.
pub async fn get_router() -> Router {
    let mut puzzle_vec = NONOGRAMMED_PUZZLE_LIST.to_vec();
    puzzle_vec.shuffle(&mut thread_rng());
    let first_puzzle = loop {
        match puzzle_vec.pop() {
            None => {
                puzzle_vec.extend_from_slice(&NONOGRAMMED_PUZZLE_LIST);
                puzzle_vec.shuffle(&mut thread_rng());
            }
            Some(puzzle_id) => {
                let puzzle = get_puzzle(puzzle_id).await;
                if let Ok(puzzle) = puzzle {
                    break puzzle;
                }
            }
        }
    };
    let rows = first_puzzle.rows.len();
    let columns = first_puzzle.columns.len();
    let (tx, rx) = watch::channel(first_puzzle);
    let duration = get_duration_for_puzzle(rows, columns);
    let state = AppState {
        puzzle: Arc::new(rx),
        nonogram: Arc::new(Mutex::new(Nonogram {
            puzzle_list: puzzle_vec,
            checkboxes: vec![CheckboxState::Empty; rows * columns],
            timer: Timer {
                start: Instant::now(),
                duration,
                join_handle: None,
            },
            state: NonogramState::Unsolved,
            puzzle_sender: tx,
        })),
        cursors: Arc::new(Mutex::new(HashMap::new())),
    };
    let state_clone = state.clone();
    let join_handle = tokio::spawn(async move {
        sleep(duration).await;
        let mut nonogram = state_clone.nonogram.lock().unwrap();
        if nonogram.state == NonogramState::Unsolved {
            nonogram.state = NonogramState::Failed;
        }
        wait_and_start_new_puzzle(state_clone.clone());
    });
    state.nonogram.lock().unwrap().timer.join_handle = Some(join_handle);
    Router::new()
        .route("/", get(index))
        .route("/htmx.js", get(htmx_minified))
        .route("/nonogram", get(nonogram))
        .route("/cursor", post(cursor))
        .route("/flag/:id", put(flag_checkbox))
        .route("/flag/:id", delete(unflag_checkbox))
        .route("/checkbox/:id", put(mark_checkbox))
        .route("/checkbox/:id", delete(unmark_checkbox))
        .with_state(state)
}

/* Main page elements */

static STYLE: &str = r#"
body {
    color: #06060c;
    background-color: #fff;
    min-height: 100vh;
}
a {
    color: #22e;
}
.hidden {
    display: none;
}
h2#congratulations {
    color: #060;
}
hr {
    margin-top: 28px;
    margin-bottom: 28px;
}
table {
    border-collapse: collapse;
    overflow: hidden;
}
tr:nth-child(5n - 3) {
    border-top: 1pt solid;
    border-top-color: #000;
}
tr th:nth-child(5n - 3), tr td:nth-child(5n - 3) {
    border-left: 1pt solid;
    border-left-color: #000;
}
th[scope="col"] {
    vertical-align: bottom;
}
th[scope="col"] > div {
    display: flex;
    flex-direction: column;
    justify-content: end;
}
th[scope="row"] {
    display: flex;
    justify-content: end;
    column-gap: 6px;
    margin-right: 2px;
}
tr:hover {
    background-color: #ff9;
}
td, th {
    position: relative;
}
td:hover::after, th:hover::after {
    content: "";
    position: absolute;
    background-color: #ff9;
    left: 0;
    top: -5023px;
    height: 13337px;
    width: 100%;
    z-index: -1;
}
.checkbox {
    position: relative;
}
.checkbox div {
    pointer-events: none;
}
.checkbox .mark {
    position: absolute;
    inset: 0;
    z-index: 2;
}
table:not(.solved) .checkbox.flagged .mark {
    background: #c76;
    border-radius: 2px;
}
table.solved .checkbox.marked .mark {
    background: #111;
}
input[type="checkbox"] {
    z-index: 1;
    transform: scale(1.4);
}
#cursors {
    position: absolute;
    z-index: 3;
    overflow: visible;
    pointer-events: none;
}
svg.cursor {
    position: absolute;
    top: 0;
    left: 0;
    opacity: 0.9;
    transition-property: transform;
    transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1);
    transition-duration: 150ms;
}
.hint {
    z-index: 4;
}
@media(prefers-color-scheme: dark) {
    body {
        color: #ccc;
        background-color: #111;
    }
    a {
        color: #4df;
    }
    h2#congratulations {
        color: #7d7;
    }
    tr:hover, td:hover::after, th:hover::after {
        background-color: #663;
    }
    tr:nth-child(5n - 3) {
        border-top-color: #fff;
    }
    tr th:nth-child(5n - 3), tr td:nth-child(5n - 3) {
        border-left-color: #fff;
    }
}
"#;

static SCRIPT: &str = r#"
document.addEventListener("contextmenu", (e) => {
    if (e.target.closest("td")) {
        e.preventDefault();
    }
});

let multipaintVersion = null;
document.addEventListener("multipaintVersion", (e) => {
    if (multipaintVersion === null) {
        multipaintVersion = e.detail.value;
    } else if (multipaintVersion !== e.detail.value) {
        location.reload();
    }
});

let isTouchDevice = (navigator.maxTouchPoints > 0 || navigator.msMaxTouchPoints > 0);
if (!isTouchDevice) {
    document.addEventListener("touchstart", (e) => {
        isTouchDevice = true;
    }, {
        once: true,
    });
}

let id = crypto.getRandomValues(new BigUint64Array(1))[0];
let table = null;
let cursors = null;
let mouseX = 0;
let mouseY = 0;
document.addEventListener("mousemove", (e) => {
    if (table === null || cursors === null) {
        table = document.querySelector("table");
        cursors = document.getElementById("cursors");
    }
    let tableBbox = table.getBoundingClientRect();
    mouseX = e.pageX - tableBbox.left;
    mouseY = e.pageY - tableBbox.top;
    cursors.style.top = tableBbox.top;
    cursors.style.left = tableBbox.left;
});

let baseTimestamp = document.timeline.currentTime;
let nonogramTimeLeft = null;
document.addEventListener("nonogramTimeLeft", (e) => {
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
"#;

async fn htmx_minified() -> &'static [u8] {
    include_bytes!("../htmx.min.js")
}

async fn index() -> Markup {
    html! {
    (DOCTYPE)
    head {
        meta charset="utf-8";
        title { "Multipaint by Numbers" }
        meta property="og:title" content="Multipaint by Numbers" {}
        meta property="og:url" content="https://multipaint.sish.top" {}
        meta property="og:description" content="Multiplayer nonogram." {}
        // script src="https://unpkg.com/htmx.org@2.0.2" integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ" crossorigin="anonymous" {}
        // script src="https://unpkg.com/htmx.org@2.0.2/dist/htmx.js" integrity="sha384-yZq+5izaUBKcRgFbxgkRYwpHhHHCpp5nseXp0MEQ1A4MTWVMnqkmcuFez8x5qfxr" crossorigin="anonymous" {}
        script src="/htmx.js" {}
        style { (PreEscaped(STYLE)) }
        script { (PreEscaped(SCRIPT)) }
    }
    body {
        #cursors hx-post="/cursor" hx-trigger="load, mousemove delay:500ms, every 1000ms" hx-vals="javascript:{id: id, mouseX: mouseX, mouseY: mouseY}" {}
        h1 { "Multipaint by Numbers" }
        hr {}
        main {
            #nonogram hx-get="/nonogram" hx-trigger="load, every 2s" {}
        }
        hr {}
        p { "Click to mark, right click to flag. Mobile devices don't support flags for now." }
        p {
            "Puzzles from "
            a href="https://nonogrammed.com/" target="_blank" {
                "Nonogrammed"
            }
            ". The source code for this website is "
            a href="https://github.com/BadMannersXYZ/htmx-ssh-games" target="_blank" {
                "on Github"
            }
            ". I know it's jank :^)"
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

async fn nonogram(State(state): State<AppState>) -> (HeaderMap, Markup) {
    let mut headers = HeaderMap::new();
    let nonogram = state.nonogram.lock().unwrap();
    let checkboxes = &nonogram.checkboxes.clone();
    let time_left = nonogram
        .timer
        .duration
        .saturating_sub(nonogram.timer.start.elapsed());
    let puzzle_state = nonogram.state;
    drop(nonogram);
    match puzzle_state {
        NonogramState::Solved(_) => {
            headers.insert(
                "HX-Trigger",
                format!(
                    "{{\"nonogramTimeLeft\": 0, \"multipaintVersion\": {}}}",
                    *VERSION
                )
                .parse()
                .unwrap(),
            );
        }
        _ => {
            headers.insert(
                "HX-Trigger",
                format!(
                    "{{\"nonogramTimeLeft\": {}, \"multipaintVersion\": {}}}",
                    time_left.as_millis(),
                    *VERSION
                )
                .parse()
                .unwrap(),
            );
        }
    }
    let puzzle = state.puzzle.borrow();
    let rows = &puzzle.rows;
    let columns = &puzzle.columns;
    let columns_len = columns.len();
    (
        headers,
        html! {
            @if matches!(puzzle_state, NonogramState::Solved(_)) {
                h2 #congratulations {
                    "Congratulations!!"
                }
            }
            @if let Some(title) = &puzzle.title {
                h3 {
                    "Puzzle: " (title) " (#" (puzzle.id) ")"
                }
            }
            @if let Some(copyright) = &puzzle.copyright {
                em .copyright {
                    (PreEscaped(copyright))
                }
            }
            (timer(
                puzzle_state,
                time_left,
            ))
            table #nonogram-table .solved[matches!(puzzle_state, NonogramState::Solved(_))] {
                tbody {
                    tr {
                        td {}
                        @for column in columns {
                            th scope="col" {
                                div {
                                    @for value in column.iter() {
                                        .hint {
                                            (value.to_string())
                                        }
                                    }
                                }
                            }
                        }
                    }
                    @for (i, row) in rows.iter().enumerate() {
                        tr {
                            th scope="row" {
                                @for value in row.iter() {
                                    .hint {
                                        (value.to_string())
                                    }
                                }
                            }
                            @let id_range = i * columns_len..(i + 1) * columns_len;
                            @let slice = &checkboxes[id_range.clone()];
                            @for (id, &state) in id_range.zip(slice) {
                                td.checkbox-cell {
                                    (checkbox(id, puzzle_state != NonogramState::Unsolved, &state))
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

fn cursor_item(cursor: &Cursor) -> Markup {
    let style = format!(
        "transform: translate({}px, {}px); color: rgb({}, {}, {});",
        cursor.position.0, cursor.position.1, cursor.color[0], cursor.color[1], cursor.color[2],
    );
    html! {
        svg .cursor id=(format!("cursor-{}", cursor.id.0)) style=(style) width="9.6014509" height="16.11743" viewBox="0 0 2.5403839 4.2644034" {
            path style="fill:currentColor;fill-opacity:1;fill-rule:evenodd;stroke:#000000;stroke-width:0.26;stroke-linejoin:round;stroke-dasharray:none;stroke-opacity:1" d="M 0.11675524,0.11673874 V 3.7065002 L 0.96455178,3.1233122 1.5307982,4.1165827 2.0934927,3.7711802 1.5414863,2.8366035 2.3925647,2.3925482 Z" {}
        }
    }
}

async fn cursor(State(state): State<AppState>, Form(payload): Form<CursorsPayload>) -> Markup {
    let position = CursorPosition(payload.mouse_x, payload.mouse_y);
    let cursor_id = CursorId(payload.id);
    let mut cursors = state.cursors.lock().unwrap();
    cursors
        .entry(cursor_id)
        .and_modify(|cursor| {
            cursor.position = position;
            cursor.modified_at = Instant::now();
        })
        .or_insert_with_key(|id| Cursor::new(*id, position));
    cursors.retain(|_, cursor| {
        Instant::now().duration_since(cursor.modified_at) <= Duration::from_secs(20)
    });
    html! {
        @for cursor_data in cursors.iter().filter(|(&id, _)| id != cursor_id) {
            (cursor_item(cursor_data.1))
        }
    }
}

fn checkbox(id: usize, disabled: bool, state: &CheckboxState) -> Markup {
    match state {
        CheckboxState::Marked => html! {
            .checkbox.marked {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[disabled] checked {}
                .mark {}
                div hx-delete=(format!("/checkbox/{id}")) hx-trigger=(format!("mousedown[buttons==1] from:#checkbox-{id}, mouseenter[buttons==1] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
        CheckboxState::Flagged if !disabled => html! {
            .checkbox.flagged {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[disabled] {}
                .mark {}
                div hx-put=(format!("/checkbox/{id}")) hx-trigger=(format!("mousedown[buttons==1] from:#checkbox-{id}, mouseenter[buttons==1] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
                div hx-delete=(format!("/flag/{id}")) hx-trigger=(format!("mousedown[buttons==2] from:#checkbox-{id}, contextmenu[isTouchDevice] from:closest .checkbox-cell, mouseenter[buttons==2] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
        _ => html! {
            .checkbox.empty {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[disabled] {}
                .mark {}
                div hx-put=(format!("/checkbox/{id}")) hx-trigger=(format!("mousedown[buttons==1] from:#checkbox-{id}, mouseenter[buttons==1] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
                div hx-put=(format!("/flag/{id}")) hx-trigger=(format!("mousedown[buttons==2] from:#checkbox-{id}, contextmenu[isTouchDevice] from:closest .checkbox-cell, mouseenter[buttons==2] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
    }
}

async fn flag_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> std::result::Result<Markup, StatusCode> {
    let mut nonogram = state.nonogram.lock().unwrap();
    let puzzle_state = nonogram.state;
    let checkboxes = &mut nonogram.checkboxes;
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if puzzle_state == NonogramState::Unsolved && checkboxes[id] == CheckboxState::Empty {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Flagged);
        Ok(checkbox(id, false, &CheckboxState::Flagged))
    } else {
        Ok(checkbox(id, true, &checkboxes[id]))
    }
}

async fn unflag_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> std::result::Result<Markup, StatusCode> {
    let mut nonogram = state.nonogram.lock().unwrap();
    let puzzle_state = nonogram.state;
    let checkboxes = &mut nonogram.checkboxes;
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if puzzle_state == NonogramState::Unsolved && checkboxes[id] == CheckboxState::Flagged {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Empty);
        Ok(checkbox(id, false, &CheckboxState::Empty))
    } else {
        Ok(checkbox(id, true, &checkboxes[id]))
    }
}

async fn mark_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> std::result::Result<Markup, StatusCode> {
    let mut nonogram = state.nonogram.lock().unwrap();
    let puzzle_state = nonogram.state;
    let timer_start = &nonogram.timer.start.clone();
    let checkboxes = &mut nonogram.checkboxes;
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if puzzle_state == NonogramState::Unsolved && checkboxes[id] != CheckboxState::Marked {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Marked);
        let checkboxes = &checkboxes.clone();
        drop(nonogram);
        if check_if_solved(&state.puzzle.borrow().solution, checkboxes, state.clone()) {
            state.nonogram.lock().unwrap().state = NonogramState::Solved(timer_start.elapsed());
            Ok(checkbox(id, true, &CheckboxState::Marked))
        } else {
            Ok(checkbox(id, false, &CheckboxState::Marked))
        }
    } else {
        Ok(checkbox(id, false, &nonogram.checkboxes[id]))
    }
}

async fn unmark_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> std::result::Result<Markup, StatusCode> {
    let mut nonogram = state.nonogram.lock().unwrap();
    let puzzle_state = nonogram.state;
    let timer_start = &nonogram.timer.start.clone();
    let checkboxes = &mut nonogram.checkboxes;
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if puzzle_state == NonogramState::Unsolved && checkboxes[id] == CheckboxState::Marked {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Empty);
        let checkboxes = &checkboxes.clone();
        drop(nonogram);
        if check_if_solved(&state.puzzle.borrow().solution, checkboxes, state.clone()) {
            state.nonogram.lock().unwrap().state = NonogramState::Solved(timer_start.elapsed());
            Ok(checkbox(id, true, &CheckboxState::Empty))
        } else {
            Ok(checkbox(id, false, &CheckboxState::Empty))
        }
    } else {
        Ok(checkbox(id, false, &nonogram.checkboxes[id]))
    }
}

/* Logic handlers */

async fn get_puzzle(puzzle_id: u32) -> Result<NonogrammedPuzzle> {
    match get_puzzle_data(puzzle_id).await {
        Err(e) => {
            warn!(error = ?e, id = puzzle_id, "Invalid puzzle.");
            Err(e)
        }
        Ok(puzzle) => {
            debug!(id = puzzle_id, "Valid puzzle.");
            Ok(puzzle)
        }
    }
}

//  5 x  5:  367s
// 10 x 10:  685s
// 20 x 20: 1277s
fn get_duration_for_puzzle(rows: usize, columns: usize) -> Duration {
    Duration::from_secs(f32::powf(20_000f32 * rows as f32 * columns as f32, 0.45) as u64)
}

fn check_if_solved(
    solution: &BitSlice<usize, Lsb0>,
    checkboxes: &[CheckboxState],
    state: AppState,
) -> bool {
    let wrong_squares = solution
        .iter()
        .zip(checkboxes.iter())
        .filter(|(solution, &state)| solution.ne(&(state == CheckboxState::Marked)))
        .count();
    let is_solved = wrong_squares == 0;
    if is_solved {
        let state_clone = state.clone();
        wait_and_start_new_puzzle(state_clone);
    } else {
        debug!("There are {wrong_squares} wrong squares!");
    }
    is_solved
}

fn wait_and_start_new_puzzle(state: AppState) {
    tokio::spawn(async move {
        sleep(Duration::from_secs(10)).await;
        // Fetch next puzzle (this is a bit inneficient)
        let next_puzzle = loop {
            let puzzle_id = state.nonogram.lock().unwrap().puzzle_list.pop().clone();
            match puzzle_id {
                None => {
                    let puzzle_vec = &mut state.nonogram.lock().unwrap().puzzle_list;
                    puzzle_vec.extend_from_slice(&NONOGRAMMED_PUZZLE_LIST);
                    puzzle_vec.shuffle(&mut thread_rng());
                }
                Some(puzzle_id) => {
                    let puzzle = get_puzzle(puzzle_id).await;
                    if let Ok(puzzle) = puzzle {
                        break puzzle;
                    }
                }
            }
        };
        let mut nonogram = state.nonogram.lock().unwrap();
        let _ = mem::replace(
            &mut nonogram.checkboxes,
            vec![CheckboxState::Empty; next_puzzle.rows.len() * next_puzzle.columns.len()],
        );
        let duration = get_duration_for_puzzle(next_puzzle.rows.len(), next_puzzle.columns.len());
        nonogram.puzzle_sender.send_replace(next_puzzle);
        nonogram.timer.duration = duration;
        nonogram.timer.start = Instant::now();
        nonogram.state = NonogramState::Unsolved;
        let state_clone = state.clone();
        let join_handle = nonogram.timer.join_handle.replace(tokio::spawn(async move {
            sleep(duration).await;
            let state = state_clone.clone();
            let mut nonogram = state.nonogram.lock().unwrap();
            if nonogram.state == NonogramState::Unsolved {
                nonogram.state = NonogramState::Failed;
            }
            wait_and_start_new_puzzle(state.clone());
        }));
        join_handle.inspect(|handle| handle.abort());
    });
}
