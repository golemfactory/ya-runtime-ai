use ya_gsb_http_proxy::monitor::{RequestMonitor, RequestsMonitor};


#[derive(Clone, Copy, Debug)]
pub struct RequestsCounter {}

impl RequestsMonitor for RequestsCounter {
    fn on_request(&mut self) -> impl RequestMonitor {
        RequestsCounter {}
    }
}

pub struct RequestCounter {}

impl RequestMonitor for RequestsCounter {
    fn on_response(self) {}
}
