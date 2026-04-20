//! Mock servers for adapter integration testing
//!
//! These mock servers simulate real backend services (Roon, UPnP)
//! allowing full integration testing without real hardware.

pub mod roon;
pub mod upnp;

pub use roon::MockRoonCore;
pub use upnp::MockUpnpRenderer;
