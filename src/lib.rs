pub mod entrypoint;
pub mod http;
pub mod nonogram;
pub mod ssh;

pub fn unwrap_infallible<T>(result: Result<T, std::convert::Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => match err {},
    }
}
