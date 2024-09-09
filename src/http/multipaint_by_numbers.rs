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
use tokio::time::sleep;
use tracing::{debug, info, warn};

#[derive(Clone)]
struct Puzzle {
    id: u32,
    title: Option<String>,
    copyright: Option<String>,
    rows: Vec<Vec<u8>>,
    columns: Vec<Vec<u8>>,
    solution: BitVec<usize, Lsb0>,
    is_solved: bool,
}

#[derive(Copy, Clone, PartialEq)]
enum CheckboxState {
    Empty,
    Flagged,
    Marked,
}

#[derive(Clone)]
struct AppState {
    checkboxes: Arc<Mutex<Vec<CheckboxState>>>,
    current_puzzle: Arc<Mutex<Puzzle>>,
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
            is_solved: false,
        })
    }
}

/// A lazily-created Router, to be used by the SSH client tunnels.
pub async fn get_router() -> Router {
    let first_puzzle = loop {
        let puzzle = get_puzzle().await;
        if let Ok(puzzle) = puzzle {
            break puzzle;
        }
    };
    info!("test");
    Router::new()
        .route("/", get(index))
        .route("/nonogram", get(nonogram))
        .route("/flag/:id", put(flag_checkbox))
        .route("/flag/:id", delete(unflag_checkbox))
        .route("/checkbox/:id", put(mark_checkbox))
        .route("/checkbox/:id", delete(unmark_checkbox))
        .with_state(AppState {
            checkboxes: Arc::new(Mutex::new(vec![
                CheckboxState::Empty;
                first_puzzle.rows.len()
                    * first_puzzle.columns.len()
            ])),
            current_puzzle: Arc::new(Mutex::new(first_puzzle)),
        })
}

fn style() -> &'static str {
    r#"
h2.congratulations {
    color: darkgreen;
}
hr {
    margin-top: 28px;
    margin-bottom: 28px;
}
table {
    border-collapse: collapse;
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
.checkbox {
    position: relative;
}
.checkbox.flagged input:not(:checked) {
    outline-style: solid;
    outline-width: 2px;
    outline-color: gray;
}
table.solved .checkbox.marked div {
    position: absolute;
    inset: 0;
    z-index: 10;
    background: black;
}
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
        }
    }
}

async fn index() -> Markup {
    html! {
        (head())
        body {
            h1 { "Multipaint by Numbers" }
            div #nonogram hx-get="/nonogram" hx-trigger="load, every 3s" {}
            hr {}
            p {
                "Puzzles from "
                a href="https://webpbn.com" target="_blank" {
                    "Web Paint-by-Number"
                }
                "."
            }
            p { "Click to mark/unmark." }
            p { "Ctrl+Click to flag." }
            p style=(PreEscaped("opacity: 0")) { "Howdy from Bad Manners!" }
        }
    }
}

async fn nonogram(State(state): State<AppState>) -> Markup {
    let puzzle = state.current_puzzle.lock().unwrap();
    let checkboxes = state.checkboxes.lock().unwrap();
    let rows = &puzzle.rows;
    let columns = &puzzle.columns;
    let columns_len = columns.len();
    let is_solved = puzzle.is_solved;
    html! {
        @if is_solved {
            h2 class="congratulations" {
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
        hr {}
        table .solved[is_solved] {
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
                                (checkbox(id, is_solved, &state))
                            }
                        }
                    }
                }
            }
        }
    }
}

fn checkbox(id: usize, is_solved: bool, state: &CheckboxState) -> Markup {
    match state {
        CheckboxState::Marked => html! {
            .checkbox.marked {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[is_solved] checked {}
                div hx-delete=(format!("/checkbox/{}", id)) hx-trigger=(format!("click from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
        CheckboxState::Flagged if !is_solved => html! {
            .checkbox.flagged {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[is_solved] {}
                div hx-put=(format!("/checkbox/{}", id)) hx-trigger=(format!("click[!ctrlKey] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
                div hx-delete=(format!("/flag/{}", id)) hx-trigger=(format!("click[ctrlKey] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
        _ => html! {
            .checkbox.empty {
                input id=(format!("checkbox-{id}")) type="checkbox" disabled[is_solved] {}
                div hx-put=(format!("/checkbox/{}", id)) hx-trigger=(format!("click[!ctrlKey] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
                div hx-put=(format!("/flag/{}", id)) hx-trigger=(format!("click[ctrlKey] from:#checkbox-{id}")) hx-swap="outerHTML" hx-target="closest .checkbox" {}
            }
        },
    }
}

async fn flag_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> Result<Markup, StatusCode> {
    let mut checkboxes = state.checkboxes.lock().unwrap();
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let is_solved = state.current_puzzle.lock().unwrap().is_solved;
    if is_solved {
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    } else {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Flagged);
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    }
}

async fn unflag_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> Result<Markup, StatusCode> {
    let mut checkboxes = state.checkboxes.lock().unwrap();
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let is_solved = state.current_puzzle.lock().unwrap().is_solved;
    if is_solved {
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    } else {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Empty);
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    }
}

async fn mark_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> Result<Markup, StatusCode> {
    let mut checkboxes = state.checkboxes.lock().unwrap();
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut puzzle = state.current_puzzle.lock().unwrap();
    let mut is_solved = puzzle.is_solved;
    if is_solved {
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    } else {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Marked);
        is_solved = check_if_solved(&puzzle.solution, &checkboxes, &state);
        puzzle.is_solved = is_solved;
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    }
}

async fn unmark_checkbox(
    State(state): State<AppState>,
    Path(id): Path<usize>,
) -> Result<Markup, StatusCode> {
    let mut puzzle = state.current_puzzle.lock().unwrap();
    let mut checkboxes = state.checkboxes.lock().unwrap();
    let mut is_solved = puzzle.is_solved;
    if checkboxes.get(id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if is_solved {
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    } else {
        let _ = std::mem::replace(&mut checkboxes[id], CheckboxState::Empty);
        is_solved = check_if_solved(&puzzle.solution, &checkboxes, &state);
        puzzle.is_solved = is_solved;
        Ok(checkbox(id, is_solved, &checkboxes[id]))
    }
}

fn check_if_solved(
    solution: &BitSlice<usize, Lsb0>,
    checkboxes: &[CheckboxState],
    state: &AppState,
) -> bool {
    let wrong_squares = solution
        .iter()
        .zip(checkboxes.iter())
        .filter(|(solution, &state)| solution.ne(&(state == CheckboxState::Marked)))
        .count();
    let is_solved = wrong_squares == 0;
    if is_solved {
        let state = state.clone();
        let current_puzzle = state.current_puzzle;
        let checkboxes = state.checkboxes;
        tokio::spawn(async move {
            sleep(Duration::from_secs(8)).await;
            // Fetch next puzzle
            let next_puzzle = loop {
                let puzzle = get_puzzle().await;
                if let Ok(puzzle) = puzzle {
                    break puzzle;
                }
            };
            let _ = mem::replace(
                checkboxes.lock().unwrap().as_mut(),
                vec![CheckboxState::Empty; next_puzzle.rows.len() * next_puzzle.columns.len()],
            );
            *current_puzzle.lock().unwrap() = next_puzzle;
        });
    } else {
        info!("Have {wrong_squares} wrong squares!");
    }
    is_solved
}
