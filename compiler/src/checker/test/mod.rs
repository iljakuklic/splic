#![allow(
    clippy::get_first,
    clippy::wildcard_enum_match_arm,
    clippy::indexing_slicing,
    clippy::similar_names,
    reason = "test code"
)]

use std::collections::HashMap;

use super::*;

use crate::common::de_bruijn;
use crate::core::{self, value, IntType, IntWidth, Name, Pat, Pi, Prim};
use crate::parser::ast::{self, BinOp, FunName, MatchArm, Phase};

mod helpers;
use helpers::*;

mod apply;
mod context;
mod literal;
mod locals;
mod matching;
mod meta;
mod signatures;
mod var;
