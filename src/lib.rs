#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![doc = include_str!("../README.md")]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[allow(rustdoc::private_intra_doc_links)]
pub mod lvm;
