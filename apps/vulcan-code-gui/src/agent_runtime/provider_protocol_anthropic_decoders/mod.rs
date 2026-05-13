#![allow(dead_code)]

//! Anthropic protocol decoders owned by the host runtime.
//! 宿主管理的 Anthropic 协议解码器。
//!
//! This module is adapted from the Anthropic SDK decoder family, but it is kept
//! inside the project so the stream parsing behavior stays inspectable and
//! independently evolvable.
//! 本模块改编自 Anthropic SDK 的解码器家族，但保留在项目内，方便流式解析行为保持可审查并可独立演进。

pub mod jsonl;
pub mod line;
pub mod sse;
