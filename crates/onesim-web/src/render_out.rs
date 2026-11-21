use crate::state::{ensure_tree_json, invalidate_frame, with, with_frame};

/// Choose what the frame getters produce: 0 = CPU pixels (`simcore_web_fb_*`, the default), 1 = an encoded display list
/// (`simcore_web_dl_*`) for the GPU executor. Resets the display-list builder (the consumer starts with no
/// resources: call this again after a device loss or re-init) and invalidates the current frame.
/// Returns 0, or 1 for an unknown mode.
#[no_mangle]
pub extern "C" fn simcore_web_set_output(mode: i32) -> i32 {
    if !(0..=1).contains(&mode) {
        return 1;
    }
    with(|s| {
        s.output = mode;
        s.builder.reset();
        s.unread = None;
        if mode == 1 {
            s.fb = Vec::new();
        } else {
            s.dl = Vec::new();
        }
        if s.scene.is_some() {
            invalidate_frame(s);
        }
    });
    0
}

/// Version of the latest scene/output pair. Reading this never builds a frame.
#[no_mangle]
pub extern "C" fn simcore_web_frame_version() -> u32 {
    with(|s| s.frame_version)
}

/// Encoded display list of the latest frame (output mode 1); valid until the next frame is built.
#[no_mangle]
pub extern "C" fn simcore_web_dl_ptr() -> *const u8 {
    with_frame(|s| {
        s.unread = None; // the consumer has it now
        s.dl.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn simcore_web_dl_len() -> usize {
    with_frame(|s| s.dl.len())
}

#[no_mangle]
pub extern "C" fn simcore_web_fb_ptr() -> *const u8 {
    with_frame(|s| s.fb.as_ptr())
}

#[no_mangle]
pub extern "C" fn simcore_web_fb_len() -> usize {
    with_frame(|s| s.fb.len())
}

#[no_mangle]
pub extern "C" fn simcore_web_fb_width() -> u32 {
    with_frame(|s| s.fb_size.0)
}

#[no_mangle]
pub extern "C" fn simcore_web_fb_height() -> u32 {
    with_frame(|s| s.fb_size.1)
}

/// Latest submitted tree JSON (debug hook); valid until the next submit.
#[no_mangle]
pub extern "C" fn simcore_web_tree_ptr() -> *const u8 {
    with(|s| {
        ensure_tree_json(s);
        s.tree_json.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn simcore_web_tree_len() -> usize {
    with(|s| {
        ensure_tree_json(s);
        s.tree_json.len()
    })
}
