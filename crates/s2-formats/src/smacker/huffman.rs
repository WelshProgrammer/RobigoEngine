//! Smacker's Huffman trees: plain 8-bit trees and 16-bit trees with a three-entry recent-value
//! cache.
//!
//! A tree is stored as a pre-order walk: a 1 bit opens a branch (its 0 side follows first), a 0
//! bit is a leaf followed by its value. Leaves of a 16-bit tree are themselves coded with two
//! 8-bit trees, one for the low byte and one for the high byte.

use super::{Error, bits::BitReader};

const LEAF: u32 = 1 << 31;
/// Marks a leaf that stands for a [`Recent`] slot, held in its low two bits.
const ESCAPE: u32 = 1 << 30;
const PAYLOAD: u32 = (1 << 16) - 1;

/// The most nodes an 8-bit tree can need: 256 leaves and 255 branches.
const MAX_SMALL_NODES: usize = 511;

/// The most nodes a 16-bit tree may have: 65,536 distinct leaves and the branches above them.
const MAX_BIG_NODES: usize = 2 * 65_536 - 1;

/// A flattened tree. Branch nodes hold the index of their 1 child (the 0 child is the next
/// node); leaf nodes hold [`LEAF`] plus a payload. An empty tree decodes to 0 without reading.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Tree {
    nodes: Vec<u32>,
}

impl Tree {
    fn build(
        bits: &mut BitReader<'_>,
        max_nodes: usize,
        mut leaf: impl FnMut(&mut BitReader<'_>) -> Result<u32, Error>,
    ) -> Result<Self, Error> {
        let mut nodes = Vec::new();
        let mut open = Vec::new();
        loop {
            if nodes.len() >= max_nodes {
                return Err(Error::TreeTooLarge);
            }
            let index = u32::try_from(nodes.len()).map_err(|_| Error::TreeTooLarge)?;
            if bits.bit()? {
                open.push(index);
                nodes.push(0);
                continue;
            }
            nodes.push(LEAF | leaf(bits)?);
            let Some(branch) = open.pop() else {
                return Ok(Self { nodes });
            };
            let next = u32::try_from(nodes.len()).map_err(|_| Error::TreeTooLarge)?;
            if let Some(slot) = nodes.get_mut(branch as usize) {
                *slot = next;
            }
        }
    }

    /// Walks from the root to a leaf and returns its payload.
    fn walk(&self, bits: &mut BitReader<'_>) -> Result<u32, Error> {
        let mut index = 0;
        loop {
            let Some(&node) = self.nodes.get(index) else {
                return Ok(0);
            };
            if node & LEAF != 0 {
                return Ok(node & !LEAF);
            }
            index = if bits.bit()? { node as usize } else { index + 1 };
        }
    }
}

/// A tree whose leaves are bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ByteTree(Tree);

impl ByteTree {
    /// Reads a presence bit and, if set, the tree and its terminating bit.
    pub(super) fn read(bits: &mut BitReader<'_>) -> Result<Self, Error> {
        if !bits.bit()? {
            return Ok(Self::default());
        }
        let tree = Tree::build(bits, MAX_SMALL_NODES, |bits| bits.u8().map(u32::from))?;
        bits.bit()?;
        Ok(Self(tree))
    }

    /// Decodes one byte.
    pub(super) fn decode(&self, bits: &mut BitReader<'_>) -> Result<u8, Error> {
        self.0.walk(bits).map(|v| v.to_le_bytes()[0])
    }
}

/// The three most recent distinct values a [`WordTree`] produced, newest first.
pub(super) type Recent = [u16; 3];

/// A tree whose leaves are 16-bit values or references into a [`Recent`] cache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct WordTree(Tree);

impl WordTree {
    /// Reads a presence bit and, if set, the two byte trees, the three escape values, the tree
    /// and its terminating bit.
    pub(super) fn read(bits: &mut BitReader<'_>) -> Result<Self, Error> {
        if !bits.bit()? {
            return Ok(Self::default());
        }
        let low = ByteTree::read(bits)?;
        let high = ByteTree::read(bits)?;
        let escapes = [bits.u16()?, bits.u16()?, bits.u16()?];
        let tree = Tree::build(bits, MAX_BIG_NODES, |bits| {
            let value = u16::from_le_bytes([low.decode(bits)?, high.decode(bits)?]);
            Ok(match escapes.iter().position(|&e| e == value) {
                Some(slot) => ESCAPE | u32::try_from(slot).unwrap_or(0),
                None => u32::from(value),
            })
        })?;
        bits.bit()?;
        Ok(Self(tree))
    }

    /// Decodes one value, resolving cache references through `recent` and updating it.
    pub(super) fn decode(&self, bits: &mut BitReader<'_>, recent: &mut Recent) -> Result<u16, Error> {
        let payload = self.0.walk(bits)?;
        let value = if payload & ESCAPE == 0 {
            u16::try_from(payload & PAYLOAD).unwrap_or(0)
        } else {
            recent.get((payload & 3) as usize).copied().unwrap_or(0)
        };
        if recent[0] != value {
            recent[2] = recent[1];
            recent[1] = recent[0];
            recent[0] = value;
        }
        Ok(value)
    }
}
