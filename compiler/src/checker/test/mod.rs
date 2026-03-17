use std::collections::HashMap;

use super::*;

use crate::core::{self, FunSig, Head, IntType, IntWidth, Pat, Prim};
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
