#![no_std]

pub mod bbqueue;
mod logger;
pub mod serial;

mod proto {
    mod proto_gen {
        #![allow(clippy::all)]
        #![allow(nonstandard_style, unused, irrefutable_let_patterns)]
        include!(concat!(env!("OUT_DIR"), "/proto.rs"));
    }

    pub use proto_gen::meshtastic_::{FromRadio, MeshPacket, ToRadio};
}
