pub mod entrypoint;
pub mod http;
pub mod ssh;
pub mod webpbpn;

pub fn unwrap_infallible<T>(result: Result<T, std::convert::Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => match err {},
    }
}
