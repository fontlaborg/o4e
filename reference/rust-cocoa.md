Short version: you have three main patterns:

1. **Call the C-based bits of macOS directly from Rust (CoreFoundation, CoreGraphics, etc.).**
2. **Use an Objective-C bridge crate to talk to Cocoa/AppKit/Foundation directly.**
3. **Let Swift/ObjC own the “app shell” and call into Rust for the heavy lifting (or vice-versa).**

I’ll walk through each and point out what’s efficient in practice.

---

## 1. FFI directly to C-style system frameworks

Quite a lot of macOS “system frameworks” are really C APIs under the hood (or have a clean C surface):

* CoreFoundation, CoreGraphics, CoreText, CoreAudio, IOKit, etc.

For those, Rust interop is straightforward and efficient:

* Use existing crates instead of rolling your own:

  * `core-foundation` for CoreFoundation types and patterns. ([Docs.rs][1])
  * There are similar crates for CoreGraphics, CoreText, CoreAudio, etc.
* Or generate bindings with `bindgen` directly from Apple headers.

Performance-wise this is great:

* FFI calls into C from Rust are essentially **zero overhead** beyond the function call itself.
* You can keep all your hot loops in Rust, only calling into Core* when you need to.

Typical setup:

```toml
# Cargo.toml
[dependencies]
core-foundation = "0.10"
```

```rust
// build.rs
fn main() {
	// Example: link to CoreFoundation framework
	println!("cargo:rustc-link-lib=framework=CoreFoundation");
}
```

Then use the crate types instead of raw pointers (CFString, CFArray, etc.).

Use this route if:

* You mostly need **non-UI** system services (files, audio, networking, etc.).
* You want maximum control and minimal magic.

---

## 2. Using Cocoa / AppKit / Foundation from Rust

For full-fat Cocoa/AppKit (NSApplication, NSWindow, etc.), you need Objective-C interop. There are two main ecosystems:

### 2.1. Modern: `objc2` + framework crates

The current “serious” stack is:

* `objc2` – safe(ish) bindings to the Objective-C runtime, with macros like `msg_send!`, `define_class!`, etc. ([Docs.rs][2])
* A set of `objc2-*` framework crates (e.g. `objc2-foundation`, `objc2-app-kit`, `objc2-core-foundation`, `objc2-metal`, etc.). ([Docs.rs][3])

These crates auto-generate bindings from Apple’s SDKs and are kept in sync with recent Xcode versions. ([Docs.rs][2])

Rough idea:

```toml
# Cargo.toml
[dependencies]
objc2 = "0.6"
objc2-foundation = "0.2"
objc2-app-kit = "0.2"
```

```rust
use objc2::rc::Retained;
use objc2::{define_class, msg_send, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSWindow, NSWindowStyleMask, NSBackingStoreType};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, ns_string};

fn main() {
	let mtm = MainThreadMarker::new().unwrap();
	let app = NSApplication::sharedApplication(mtm);

	// Allocate a window
	let window = unsafe {
		NSWindow::initWithContentRect_styleMask_backing_defer(
			NSWindow::alloc(mtm),
			NSRect::new(
				NSPoint::new(0.0, 0.0),
				NSSize::new(400.0, 300.0),
			),
			NSWindowStyleMask::Titled,
			NSBackingStoreType::Buffered,
			false,
		)
	};
	window.setTitle(ns_string!("Hello from Rust + Cocoa"));
	window.center();
	window.makeKeyAndOrderFront(None);

	app.run();
}
```

This is basically what the official `objc2` docs do in their “Hello World” example. ([Docs.rs][2])

**Pros:**

* Pretty direct, thin binding to the real APIs → very little overhead.
* Quite safe: framework crates model ownership and threading (MainThreadMarker etc.) to avoid UB.
* Actively maintained.

**Cons:**

* The API feels like “Rust with Objective-C sauce”; it’s not as simple as writing Swift.
* You’re still exposed to Cocoa concepts (delegates, run loops, retain cycles, etc.).

### 2.2. Older: `cocoa` / `objc` / `block` / `cacao`

History/alternatives:

* `cocoa` crate – older Rust bindings to Cocoa, widely used for early macOS GUIs in Rust. ([Crates][4])
* `objc` + `block` crates – lower-level runtime and blocks bindings that `cocoa` and others are built on.
* `cacao` – a higher-level wrapper that tries to make AppKit / UIKit feel more Swifty from Rust, with safe types and idiomatic patterns. ([GitHub][5])

These are still usable, but:

* `objc2` + friends are the current “clean design” direction.
* `cacao` is nice if you like a more opinionated abstraction, but it’s still somewhat experimental.

**When to choose this route:**

* You want a **pure Rust** Cocoa app (no separate Swift/ObjC project).
* You’re okay living close to the metal in terms of API complexity.
* You care about longer-term maintainability and are happy to bet on `objc2`.

---

## 3. Native app shell in Swift / ObjC, Rust as the engine

For many GUI apps, the most *pragmatic* and efficient path is:

> **UI in Swift/ObjC, logic in Rust.**

You then connect them through a C ABI:

1. **Build Rust as a static or dynamic library**:

   ```toml
   [lib]
   crate-type = ["staticlib"]   # or "cdylib"
   ```

   ```rust
   #[no_mangle]
   pub extern "C" fn do_heavy_thing(x: i32) -> i32 {
	   // compute stuff in Rust
	   x * 2
   }
   ```

2. **Expose a C interface on the Swift side**:

   * Mark Swift functions with `@_cdecl` so they’re callable as C functions, or wrap Swift in an Objective-C class and expose that via a generated header (`*-Swift.h`). ([Swift Forums][6])

3. **Use a bridge header and Xcode’s normal integration** to wire Swift/ObjC <→ Rust via those C-ABI functions.

A concrete write-up of this workflow (for Rust calling Swift) is here, using a Swift shim built from Cargo’s `build.rs` and C-style exported functions. ([Haim][7])

**Why this can be the “most efficient” overall:**

* You write your GUI in the language Cocoa tooling is built around (Interface Builder, storyboards, SwiftUI, etc.).
* Rust stays focused on high-performance logic, algorithms, data crunching.
* Crossing the C FFI boundary occasionally is cheap; you just don’t want to do it millions of times per frame.

This is the pattern I’d recommend if you’re building:

* A “proper” macOS desktop app with menus, windows, document architecture.
* Anything where Xcode and SwiftUI would save you a lot of pain.

---

## 4. Practical tips for efficiency

Regardless of which path you take:

1. **Batch work on the Rust side.**
   Don’t call a Rust function per pixel from Cocoa, or per element from a tight Swift loop. Move big chunks of work to Rust, then pass results back.

2. **Respect the main thread.**
   UI objects (NSWindow, NSView, etc.) must be used on the main thread; use Rust threads for background work and hop back via `dispatch_async` or equivalents (`dispatch2` or `objc2` helpers).

3. **Use `build.rs` to link frameworks once.**
   Instead of passing `-framework` flags manually every time, stick them in `build.rs`:

   ```rust
   println!("cargo:rustc-link-lib=framework=AppKit");
   println!("cargo:rustc-link-lib=framework=Foundation");
   ```

4. **If you don’t care about “native Cocoa” but just need a window**, consider `winit` / `tao` / egui etc. They use Cocoa under the hood on macOS, but you don’t touch it directly; this is often the fastest path to productive GUI work.

---

## What I’d do, concretely

* **For a mostly-Rust app that just needs a native window:** use `objc2 + objc2-app-kit` (modern) or a higher-level crate like `cacao`.
* **For a big Cocoa / SwiftUI app where UI/UX matters:**

  * Build the app in Swift,
  * Put Rust in a static library,
  * Call it from Swift via C-ABI functions.

If you tell me what kind of app you’re thinking of (CLI with small Cocoa bits? Full GUI editor? Menu bar helper?), I can sketch a minimal project layout for that specific scenario.

[1]: https://docs.rs/core-foundation?utm_source=chatgpt.com "core_foundation - Rust"
[2]: https://docs.rs/objc2/ "objc2 - Rust"
[3]: https://docs.rs/objc2/?utm_source=chatgpt.com "Crate objc2 - Rust"
[4]: https://crates.io/crates/cocoa?utm_source=chatgpt.com "cocoa - crates.io: Rust Package Registry"
[5]: https://github.com/ryanmcgrath/cacao?utm_source=chatgpt.com "ryanmcgrath/cacao: Rust bindings for AppKit (macOS) and ..."
[6]: https://forums.swift.org/t/best-way-to-call-a-swift-function-from-c/9829?utm_source=chatgpt.com "Best way to call a Swift function from C?"
[7]: https://haim.dev/posts/2020-09-10-linking-swift-code-into-rust-app/?utm_source=chatgpt.com "Linking Swift code into a Rust app - Haim Gelfenbeyn's Blog"
