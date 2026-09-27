//! Request identity keeps stale frontend frames from reopening or moving a menu.
#[derive(Default)]
pub struct TrayRequest {
    pub ready: bool,
    pub requested: bool,
    pub presented: bool,
    pub generation: u64,
}
impl TrayRequest {
    pub fn show(&mut self) {
        self.generation += 1;
        self.requested = true;
    }
    pub fn hide(&mut self) {
        self.requested = false;
        self.presented = false;
    }
    pub fn accept_frame(&mut self, generation: u64) -> bool {
        if !self.ready || !self.requested || generation != self.generation {
            return false;
        }
        self.presented = true;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_and_stale_frames_never_expose_or_reopen_the_menu() {
        let mut request = TrayRequest::default();
        request.show();
        assert!(!request.accept_frame(1));
        request.ready = true;
        request.show();
        assert!(!request.accept_frame(1));
        assert!(request.accept_frame(2));
        request.hide();
        assert!(!request.accept_frame(2));
        request.show();
        assert!(!request.accept_frame(2));
        assert!(request.accept_frame(3));
    }
}
