//! Top header (thin module): the drawing lives in `app::render_header`
//! so `apps/darb` and the tests share one implementation. This module
//! exists to keep the components tree symmetrical with the reference
//! structure (header / sidebar / ai_session / context / status).

pub use crate::app::render_header;
