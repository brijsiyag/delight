//! The blinking cursor: a tiny entity the editor observes. It toggles every
//! [`INTERVAL`] while the editor is focused and stays solid while the user
//! types ([`BlinkCursor::pause`]). An epoch counter retires stale timers.

use std::time::Duration;

use gpui::{Context, Task};

const INTERVAL: Duration = Duration::from_millis(500);
/// How long the cursor stays solid after a keystroke.
const PAUSE: Duration = Duration::from_millis(300);

pub struct BlinkCursor {
    visible: bool,
    paused: bool,
    epoch: usize,
    _timer: Task<()>,
}

impl BlinkCursor {
    pub fn new() -> Self {
        Self { visible: false, paused: false, epoch: 0, _timer: Task::ready(()) }
    }

    pub fn visible(&self) -> bool {
        self.paused || self.visible
    }

    /// Starts blinking, cursor shown (on focus).
    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        let epoch = self.next_epoch();
        self.blink(epoch, cx);
    }

    /// Stops blinking, cursor hidden (on blur).
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.next_epoch();
        self.visible = false;
        self.paused = false;
        self._timer = Task::ready(());
        cx.notify();
    }

    /// Shows the cursor solid, and resumes blinking once typing pauses.
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.paused = true;
        cx.notify();
        let epoch = self.next_epoch();
        self._timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PAUSE).await;
            this.update(cx, |this, cx| {
                this.paused = false;
                this.visible = false;
                this.blink(epoch, cx);
            })
            .ok();
        });
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch += 1;
        self.epoch
    }

    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch != self.epoch {
            return;
        }
        self.visible = !self.visible;
        cx.notify();
        self._timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(INTERVAL).await;
            this.update(cx, |this, cx| this.blink(epoch, cx)).ok();
        });
    }
}
