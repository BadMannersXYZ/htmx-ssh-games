use std::{
    mem,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, State},
    routing::{delete, get, put},
    Router,
};
use bitvec::{bitvec, order::Lsb0, slice::BitSlice, vec::BitVec};
use hyper::StatusCode;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use reqwest::redirect::Policy;
use tokio::{
    task::JoinHandle,
    time::{sleep, Instant},
};
use tracing::{debug, info, warn};

#[derive(Copy, Clone, PartialEq)]
enum NonogramState {
    Unsolved,
    Solved(Duration),
    Failed,
}

#[derive(Clone)]
struct Puzzle {
    id: u32,
    title: Option<String>,
    copyright: Option<String>,
    rows: Vec<Vec<u8>>,
    columns: Vec<Vec<u8>>,
    solution: BitVec<usize, Lsb0>,
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
    state: NonogramState,
    puzzle: Puzzle,
    checkboxes: Vec<CheckboxState>,
    timer: Timer,
}

#[derive(Clone)]
struct AppState {
    nonogram: Arc<Mutex<Nonogram>>,
}

enum GetPuzzleState {
    Start,
    ReadingRows,
    ReadingColumns,
}

async fn get_puzzle() -> Result<Puzzle> {
    let client = reqwest::ClientBuilder::new()
        .redirect(Policy::none())
        .build()
        .unwrap();
    let redirect_response = client
        .post("https://webpbn.com/random.cgi")
        .form(&[
            ("sid", ""),
            ("go", "1"),
            ("psize", "1"),
            ("pcolor", "1"),
            ("pmulti", "1"),
            ("pguess", "1"),
            ("save", "1"),
        ])
        .send()
        .await
        .unwrap();
    let location = redirect_response.headers().get("location").unwrap();
    let id = location
        .to_str()
        .unwrap()
        .split_once("id=")
        .unwrap()
        .1
        .split('&')
        .next()
        .unwrap()
        .parse::<u32>()
        .unwrap();
    debug!(id = id, "Fetching puzzle...");

    let client = reqwest::Client::new();
    let export_response = client
        .post(format!("https://webpbn.com/export.cgi/webpbn{:06}.non", id))
        .form(&[
            ("go", "1"),
            ("sid", ""),
            ("id", &id.to_string()),
            ("xml_clue", "on"),
            ("xml_soln", "on"),
            ("fmt", "ss"),
            ("ss_soln", "on"),
            ("sg_clue", "on"),
            ("sg_soln", "on"),
        ])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let mut title = None;
    let mut copyright = None;
    let mut rows = vec![];
    let mut columns = vec![];
    let mut solution = bitvec![];
    let mut state = GetPuzzleState::Start;
    for line in export_response.lines() {
        match state {
            GetPuzzleState::Start => {
                if line.starts_with("title") {
                    let mut iter = line.splitn(3, '"');
                    iter.next().unwrap();
                    title = Some(String::from(iter.next().unwrap()));
                } else if line.starts_with("copyright") {
                    let mut iter = line.splitn(3, '"');
                    iter.next().unwrap();
                    copyright = Some(String::from(iter.next().unwrap()));
                } else if line.starts_with("rows") {
                    state = GetPuzzleState::ReadingRows;
                } else if line.starts_with("columns") {
                    state = GetPuzzleState::ReadingColumns;
                } else if line.starts_with("goal") {
                    let mut iter = line.splitn(3, '"');
                    iter.next().unwrap();
                    solution.extend(iter.next().unwrap().chars().map(|char| char == '1'));
                } else if line.starts_with("copyright") {
                    let mut iter = line.splitn(3, '"');
                    iter.next().unwrap();
                    title = Some(String::from(iter.next().unwrap()));
                }
            }
            GetPuzzleState::ReadingRows => {
                if line.is_empty() {
                    state = GetPuzzleState::Start;
                } else {
                    let row = line
                        .split(',')
                        .map(|text| str::parse::<u8>(text).unwrap())
                        .filter(|&value| value > 0)
                        .collect::<Vec<_>>();
                    rows.push(row);
                }
            }
            GetPuzzleState::ReadingColumns => {
                if line.is_empty() {
                    state = GetPuzzleState::Start;
                } else {
                    let column = line
                        .split(',')
                        .map(|text| str::parse::<u8>(text).unwrap())
                        .filter(|&value| value > 0)
                        .collect::<Vec<_>>();
                    columns.push(column);
                }
            }
        }
    }
    if rows.is_empty() || columns.is_empty() || solution.is_empty() {
        warn!(id = id, "Invalid puzzle.");
        Err(anyhow!("Invalid puzzle"))
    } else {
        info!(id = id, "Valid puzzle.");
        Ok(Puzzle {
            id,
            title,
            copyright,
            rows,
            columns,
            solution,
        })
    }
}

//  5 x  5:  393s
// 10 x 10:  891s
// 20 x 20: 2019s
fn get_duration_for_puzzle(rows: usize, columns: usize) -> Duration {
    Duration::from_secs(f32::powf(1000f32 * rows as f32 * columns as f32, 0.59) as u64)
}

/// A lazily-created Router, to be used by the SSH client tunnels.
pub async fn get_router() -> Router {
    let first_puzzle = loop {
        let puzzle = get_puzzle().await;
        if let Ok(puzzle) = puzzle {
            break puzzle;
        }
    };
    let duration = get_duration_for_puzzle(first_puzzle.rows.len(), first_puzzle.columns.len());
    let state = AppState {
        nonogram: Arc::new(Mutex::new(Nonogram {
            checkboxes: vec![
                CheckboxState::Empty;
                first_puzzle.rows.len() * first_puzzle.columns.len()
            ],
            timer: Timer {
                start: Instant::now(),
                duration,
                join_handle: None,
            },
            state: NonogramState::Unsolved,
            puzzle: first_puzzle,
        })),
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
    info!("test");
    Router::new()
        .route("/", get(index))
        .route("/nonogram", get(nonogram))
        .route("/timer", get(timer))
        .route("/flag/:id", put(flag_checkbox))
        .route("/flag/:id", delete(unflag_checkbox))
        .route("/checkbox/:id", put(mark_checkbox))
        .route("/checkbox/:id", delete(unmark_checkbox))
        .with_state(state)
}

fn style() -> &'static str {
    r#"
h2#congratulations {
    color: darkgreen;
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
    border-top: 1pt solid black;
}
tr th:nth-child(5n - 3), tr td:nth-child(5n - 3) {
    border-left: 1pt solid black;
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
td:hover::after,
th:hover::after {
  content: "";
  position: absolute;
  background-color: #ff9;
  left: 0;
  top: -5000px;
  height: 10000px;
  width: 100%;
  z-index: -1;
}
.checkbox {
    position: relative;
}
.checkbox.flagged input:not(:checked) {
    outline-style: solid;
    outline-width: 2px;
    outline-color: #c76;
}
table.solved .checkbox.marked div {
    position: absolute;
    inset: 0;
    z-index: 2;
    background: black;
}
input[type="checkbox"] {
    transform: scale(1.33);
}
"#
}

fn script() -> &'static str {
    r#"
document.oncontextmenu = (e) => {
    if (e.target.closest('.checkbox')) {
        e.preventDefault();
    }
};
"#
}

fn head() -> Markup {
    html! {
        (DOCTYPE)
        head {
            meta charset="utf-8";
            title { "Multipaint by Numbers" }
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
        h1 { "Multipaint by Numbers" }
        main {
            #nonogram hx-get="/nonogram" hx-trigger="load, every 3s" {}
        }
        hr {}
        p { "Click to mark/unmark. Ctrl+Click to flag." }
        p {
            "Hey there! If you'd like to tip me so I can buy better servers or add more features, check out my "
            a href="https://ko-fi.com/badmanners" {
                "Ko-fi"
            }
            ". Thanks!"
        }
        }
        p {
            "The puzzles are from "
            a href="https://webpbn.com" target="_blank" {
                "Web Paint-by-Number"
            }
            ". The source code for this website is "
            a href="https://github.com/BadMannersXYZ/htmx-ssh-games" target="_blank" {
                "on Github"
            }
            "."
        }
    }
}

async fn timer(State(state): State<AppState>) -> Markup {
    let nonogram = state.nonogram.lock().unwrap();
    timer_inner(nonogram.state, &nonogram.timer)
}

fn timer_inner(puzzle_state: NonogramState, timer: &Timer) -> Markup {
    if let NonogramState::Solved(success) = puzzle_state {
        let secs = success.as_secs();
        return html! {
            p {
                "Solved in " (format!("{}:{:02}", secs / 60, secs % 60)) "!"
            }
        };
    };
    let time_left = timer.duration.saturating_sub(timer.start.elapsed());
    if time_left == Duration::ZERO {
        html! {
            p {
                "Time's up!"
            }
        }
    } else {
        let secs = time_left.as_secs();
        html! {
            p hx-get="/timer" hx-trigger="every 1s" hx-swap="outerHTML" {
                "Time left: " (format!("{}:{:02}", secs / 60, secs % 60))
            }
        }
    }
}

async fn nonogram(State(state): State<AppState>) -> Markup {
    let nonogram = state.nonogram.lock().unwrap();
    let puzzle = &nonogram.puzzle;
    let checkboxes = &nonogram.checkboxes;
    let timer = &nonogram.timer;
    let puzzle_state = nonogram.state;
    let rows = &puzzle.rows;
    let columns = &puzzle.columns;
    let columns_len = columns.len();
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
        (timer_inner(
            puzzle_state,
            timer,
        ))
        hr {}
        table .solved[matches!(puzzle_state, NonogramState::Solved(_))] {
            tbody {
                tr {
                    td {}
                    @for column in columns {
                        th scope="col" {
                            div {
                                @for value in column.iter() {
                                    div {
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
                                div {
                                    (value.to_string())
                                }
                            }
                        }
                        @let id_range = i * columns_len..(i + 1) * columns_len;
                        @let slice = &checkboxes[id_range.clone()];
                        @for (id, &state) in id_range.zip(slice) {
                            td {
                                (checkbox(id, puzzle_state != NonogramState::Unsolved, &state))
                            }
                        }
                    }
                }
            }
        }
    }
}

fn checkbox(id: usize, disabled: bool, state: &CheckboxState) -> Markup {
    match state {
        CheckboxState::Marked => html! {
            .checkbox.marked {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[disabled] checked {}
                div hx-delete=(format!("/checkbox/{}", id)) hx-trigger=(format!("click from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
        CheckboxState::Flagged if !disabled => html! {
            .checkbox.flagged {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[disabled] {}
                div hx-put=(format!("/checkbox/{}", id)) hx-trigger=(format!("click[!ctrlKey] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
                div hx-delete=(format!("/flag/{}", id)) hx-trigger=(format!("click[ctrlKey] from:#checkbox-{id},contextmenu from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
        _ => html! {
            .checkbox.empty {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[disabled] {}
                div hx-put=(format!("/checkbox/{}", id)) hx-trigger=(format!("click[!ctrlKey] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
                div hx-put=(format!("/flag/{}", id)) hx-trigger=(format!("click[ctrlKey] from:#checkbox-{id}, contextmenu from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
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
    if puzzle_state == NonogramState::Unsolved {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Flagged);
        Ok(checkbox(id, false, &checkboxes[id]))
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
    if puzzle_state == NonogramState::Unsolved {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Empty);
        Ok(checkbox(id, false, &checkboxes[id]))
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
    let checkboxes = &nonogram.checkboxes.clone();
    let puzzle = &nonogram.puzzle.clone();
    let timer_start = &nonogram.timer.start.clone();
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if puzzle_state == NonogramState::Unsolved {
        let _ = std::mem::replace(&mut nonogram.checkboxes[id], CheckboxState::Marked);
        if check_if_solved(&puzzle.solution, checkboxes, state.clone()) {
            nonogram.state = NonogramState::Solved(timer_start.elapsed());
            Ok(checkbox(id, true, &CheckboxState::Marked))
        } else {
            Ok(checkbox(id, false, &CheckboxState::Marked))
        }
    } else {
        Ok(checkbox(id, false, &checkboxes[id]))
    }
}

async fn unmark_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> std::result::Result<Markup, StatusCode> {
    let mut nonogram = state.nonogram.lock().unwrap();
    let puzzle_state = nonogram.state;
    let checkboxes = &nonogram.checkboxes.clone();
    let puzzle = &nonogram.puzzle.clone();
    let timer_start = &nonogram.timer.start.clone();
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if puzzle_state == NonogramState::Unsolved {
        let _ = std::mem::replace(&mut nonogram.checkboxes[id], CheckboxState::Empty);
        if check_if_solved(&puzzle.solution, checkboxes, state.clone()) {
            nonogram.state = NonogramState::Solved(timer_start.elapsed());
            Ok(checkbox(id, true, &CheckboxState::Empty))
        } else {
            Ok(checkbox(id, false, &CheckboxState::Empty))
        }
    } else {
        Ok(checkbox(id, false, &checkboxes[id]))
    }
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
        info!("There are {wrong_squares} wrong squares!");
    }
    is_solved
}

fn wait_and_start_new_puzzle(state: AppState) {
    tokio::spawn(async move {
        sleep(Duration::from_secs(10)).await;
        // Fetch next puzzle
        let next_puzzle = loop {
            let puzzle = get_puzzle().await;
            if let Ok(puzzle) = puzzle {
                break puzzle;
            }
        };
        let mut nonogram = state.nonogram.lock().unwrap();
        let _ = mem::replace(
            &mut nonogram.checkboxes,
            vec![CheckboxState::Empty; next_puzzle.rows.len() * next_puzzle.columns.len()],
        );
        let duration = get_duration_for_puzzle(next_puzzle.rows.len(), next_puzzle.columns.len());
        nonogram.puzzle = next_puzzle;
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
