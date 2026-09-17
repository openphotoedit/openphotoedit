// SPDX-License-Identifier: LGPL-2.1
// Local change (OpenPhotoEdit): `std::time::Instant::now()` panics on
// wasm32-unknown-unknown ("time not implemented on this platform"). rawler only
// uses it for debug timing logs, so on that target it becomes a no-op clock.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use std::time::Instant;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[derive(Clone, Copy, Debug)]
pub struct Instant;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
impl Instant {
  pub fn now() -> Self {
    Instant
  }
  pub fn elapsed(&self) -> std::time::Duration {
    std::time::Duration::ZERO
  }
}
