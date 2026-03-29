#![allow(
    clippy::get_first,
    clippy::wildcard_enum_match_arm,
    clippy::indexing_slicing
)]

use std::collections::HashMap;

use super::*;

use crate::core::{self, IntType, IntWidth, Ix, Lvl, Name, Pat, Pi, Prim, value};
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
