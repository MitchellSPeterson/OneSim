#ifndef SIMCORE_RUNTIME_H
#define SIMCORE_RUNTIME_H

/*
 * OneSim guest runtime C ABI (protocol.md section C).
 *
 * Threading: single-threaded. Call every function from the same thread; the
 * callbacks are invoked on the thread that is inside simcore_runtime_run().
 * Callbacks may call simcore_submit_tree / simcore_report_error.
 *
 * Ownership: Rust never retains pointers passed in. Strings/buffers given to
 * Rust are copied before the call returns; the caller keeps ownership. Strings
 * passed to callbacks (`appearance`) are owned by Rust and valid only for the
 * duration of the callback. simcore_last_error() returns a pointer owned by
 * Rust, valid until the next simcore_* call on the same thread; do not free it.
 */
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*simcore_action_cb)(void *ctx, uint64_t action_id);
typedef void (*simcore_env_cb)(void *ctx, const char *appearance, double width, double height);

/* text_input: `text` is the full new field contents (NUL-terminated UTF-8, owned by Rust,
 * valid only during the callback). Version-gated like actions. */
typedef void (*simcore_text_cb)(void *ctx, uint64_t field_id, const char *text);

/* gesture (protocol section L): one touch event on views with SwiftUI gestures. phase: 0 start, 1 move,
 * 2 end, 3 cancel; `time` in ms. `points` holds `npoints` (x, y) pairs in screen points; `keys` the
 * `ntargets` gesture keys, innermost first, and `frames` their (x, y, w, h) frames, 4 doubles each. All
 * owned by Rust, valid only during the callback. Not version-gated: keys are structural paths. */
typedef void (*simcore_gesture_cb)(void *ctx, int32_t phase, double time, const double *points, size_t npoints,
                                   const char *const *keys, const double *frames, size_t ntargets);

/* Call once, first. dup()s fd 1 as the protocol channel, dup2(2,1), sends hello.
 * Returns 0 on success, nonzero on failure (see simcore_last_error). */
int32_t simcore_runtime_init(void);

/* ctx is passed back verbatim and must stay valid until simcore_runtime_run returns. */
void simcore_runtime_set_callbacks(void *ctx, simcore_action_cb on_action, simcore_env_cb on_env);

/* Optional. Registers the text_input callback; `ctx` is the one passed to
 * simcore_runtime_set_callbacks. Independent of set_callbacks (existing ABI unchanged). */
void simcore_runtime_set_text_callback(simcore_text_cb on_text);

/* Optional. Registers the gesture callback; `ctx` is the one passed to simcore_runtime_set_callbacks. */
void simcore_runtime_set_gesture_callback(simcore_gesture_cb on_gesture);

/* Send a view tree (UTF-8 JSON ViewNode, not NUL-terminated). Validated as JSON
 * and copied. `version` must be strictly increasing. Returns 0 on success. */
int32_t simcore_submit_tree(const uint8_t *json, size_t len, uint64_t version);

/* Report a non-fatal error (NUL-terminated UTF-8) to the host. Copied. */
void simcore_report_error(const char *msg);

/* Blocking event loop. Actions whose version != last submitted tree version are
 * dropped. Returns 0 after `shutdown` or stdin EOF, nonzero on I/O failure. */
int32_t simcore_runtime_run(void);

/* Thread-local last error message, or NULL if none. Do not free. */
const char *simcore_last_error(void);

#ifdef __cplusplus
}
#endif
#endif
