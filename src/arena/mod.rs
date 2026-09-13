// MIT License
//
// Copyright (C) INFINI Labs & INFINI LIMITED. <hello@infini.ltd>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Chunked arena allocator.
//!
//! Phase-3 unlock item ④ (2026-09-13): interior mutability removed. The four
//! `RefCell`s (chunks / snapshot offsets / counters) are plain fields now —
//! appends take `&mut self`, reads take `&self`, and the borrow checker
//! enforces the aliasing discipline that previously relied on every caller
//! living on the same thread. As a side effect `Arena<T>` is `Send + Sync`
//! whenever `T` is, the returned `&T`/`&mut T`/snapshot references carry real
//! (sound) lifetimes instead of transmuted ones, and every `unsafe` block is
//! gone. Callers that allocated through `&self` must switch to `&mut self`
//! access (in pizza-engine every append path already holds `&mut` on the
//! owning store).

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;
use core::mem::size_of;

pub struct Arena<T> {
    max_items: usize,
    max_memory_bytes: usize,
    chunks: Vec<Vec<T>>,
    snapshot_offsets: Vec<(usize, usize)>, // Stores (last_chunk_index, last_chunk_len)
    total_items: usize,
    total_memory_used: usize,
}

impl<T> fmt::Debug for Arena<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Arena")
            .field("max_items", &self.max_items)
            .field("max_memory_bytes", &self.max_memory_bytes)
            .field("chunks", &self.chunks)
            .field("snapshot_offsets", &self.snapshot_offsets)
            .field("total_items", &self.total_items)
            .field("total_memory_used", &self.total_memory_used)
            .finish()
    }
}

impl<T> Arena<T> {
    pub fn new(initial_item_capacity: usize, max_items: usize, max_memory_bytes: usize) -> Self {
        Self {
            chunks: vec![Vec::with_capacity(initial_item_capacity)],
            snapshot_offsets: Vec::new(),
            max_items,
            max_memory_bytes,
            total_items: 0,
            total_memory_used: 0,
        }
    }

    pub fn must_alloc(&mut self, value: T) -> &mut T {
        self.alloc(value).unwrap()
    }

    pub fn alloc(&mut self, value: T) -> Result<&mut T, String> {
        // Call the `alloc` method to do the allocation and return only the reference
        let (_, _, v) = self.advanced_alloc(value)?;
        Ok(v)
    }

    pub fn advanced_alloc(&mut self, value: T) -> Result<(usize, usize, &mut T), String> {
        let last_index = self.chunks.len() - 1;
        let element_size = size_of::<T>();

        if self.total_items < self.max_items
            && self.total_memory_used + element_size <= self.max_memory_bytes
        {
            let (chunk_index, element_index) =
                if self.chunks[last_index].len() < self.chunks[last_index].capacity() {
                    // Add to the last chunk
                    self.chunks[last_index].push(value);
                    (last_index, self.chunks[last_index].len() - 1)
                } else {
                    // Create a new chunk with double the capacity of the last chunk
                    let new_capacity = self.chunks[last_index].capacity() * 2;
                    let mut new_chunk = Vec::with_capacity(new_capacity);
                    new_chunk.push(value);
                    self.chunks.push(new_chunk);
                    let new_chunk_index = self.chunks.len() - 1;
                    (new_chunk_index, 0)
                };

            self.total_items += 1;
            self.total_memory_used += element_size;

            // Return a mutable reference to the newly pushed element along with the indices
            let chunk = &mut self.chunks[chunk_index];
            Ok((chunk_index, element_index, &mut chunk[element_index]))
        } else {
            Err(format!(
                "Arena capacity exceeded, {}/{}, {}/{}",
                self.total_items, self.max_items, self.total_memory_used, self.max_memory_bytes
            ))
        }
    }

    // Retrieve a reference to an element using its index
    pub fn get(&self, chunk_index: usize, element_index: usize) -> Option<&T> {
        // Ensure the chunk_index and element_index are within bounds
        self.chunks
            .get(chunk_index)
            .and_then(|chunk| chunk.get(element_index))
    }

    // Retrieve a mutable reference to an element using its index
    pub fn get_mut(&mut self, chunk_index: usize, element_index: usize) -> Option<&mut T> {
        // Ensure the chunk_index and element_index are within bounds
        self.chunks
            .get_mut(chunk_index)
            .and_then(|chunk| chunk.get_mut(element_index))
    }

    pub fn total_chunks(&self) -> usize {
        self.chunks.len()
    }

    pub fn total_items(&self) -> usize {
        self.total_items
    }

    pub fn total_memory_usage(&self) -> usize {
        self.total_memory_used
    }

    pub fn snapshot(&mut self) -> usize {
        let last_chunk_index = self.chunks.len() - 1;
        let last_chunk_len = self.chunks[last_chunk_index].len();
        self.snapshot_offsets.push((last_chunk_index, last_chunk_len));
        self.snapshot_offsets.len() - 1 // Return the snapshot ID
    }

    pub fn get_snapshot(&self, snapshot: usize) -> Vec<&T> {
        let (last_chunk_index, last_chunk_len) = self.snapshot_offsets[snapshot];

        let mut result = Vec::new();
        for chunk in &self.chunks[..last_chunk_index] {
            result.extend(chunk.iter());
        }
        let last_chunk = &self.chunks[last_chunk_index];
        result.extend(last_chunk.iter().take(last_chunk_len));

        result
    }

    pub fn reset(&mut self) {
        self.chunks.clear();
        self.chunks.push(Vec::with_capacity(1)); // Restart with initial capacity
        self.snapshot_offsets.clear();
        self.total_items = 0;
        self.total_memory_used = 0;
    }
}

pub struct ArenaIterator<'a, T> {
    chunks: &'a [Vec<T>],
    pub batch_size: usize,
    chunk_index: usize,
    item_index: usize,
    pub next_value: Option<T>, //for VectorIterator only
}

impl<'a, T> Iterator for ArenaIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.chunk_index >= self.chunks.len() {
                return None;
            }

            let chunk = &self.chunks[self.chunk_index];

            if self.item_index < chunk.len() {
                let item = &chunk[self.item_index];
                self.item_index += 1;
                return Some(item);
            } else {
                self.chunk_index += 1;
                self.item_index = 0;
            }
        }
    }
}

impl<T> Arena<T> {
    pub fn iter_with_batch_size(&self, batch_size: usize) -> ArenaIterator<'_, T> {
        ArenaIterator {
            chunks: &self.chunks,
            chunk_index: 0,
            item_index: 0,
            batch_size,
            next_value: None,
        }
    }

    pub fn iter(&self) -> ArenaIterator<'_, T> {
        self.iter_with_batch_size(512)
    }
}

use serde::de::MapAccess;
use serde::de::Visitor;
use serde::de::{self};
use serde::ser::SerializeStruct;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

impl<T: Serialize> Serialize for Arena<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // We need to manually serialize each field
        let mut state = serializer.serialize_struct("Arena", 6)?;
        state.serialize_field("max_items", &self.max_items)?;
        state.serialize_field("max_memory_bytes", &self.max_memory_bytes)?;
        state.serialize_field("chunks", &self.chunks)?;
        state.serialize_field("snapshot_offsets", &self.snapshot_offsets)?;
        state.serialize_field("total_items", &self.total_items)?;
        state.serialize_field("total_memory_used", &self.total_memory_used)?;
        state.end()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Arena<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            MaxItems,
            MaxMemoryBytes,
            Chunks,
            SnapshotOffsets,
            TotalItems,
            TotalMemoryUsed,
        }

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`max_items`, `max_memory_bytes`, `chunks`, `snapshot_offsets`, `total_items`, or `total_memory_used`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "max_items" => Ok(Field::MaxItems),
                            "max_memory_bytes" => Ok(Field::MaxMemoryBytes),
                            "chunks" => Ok(Field::Chunks),
                            "snapshot_offsets" => Ok(Field::SnapshotOffsets),
                            "total_items" => Ok(Field::TotalItems),
                            "total_memory_used" => Ok(Field::TotalMemoryUsed),
                            _ => Err(de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct ArenaVisitor<T>(PhantomData<fn() -> Arena<T>>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ArenaVisitor<T> {
            type Value = Arena<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Arena")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Arena<T>, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut max_items = None;
                let mut max_memory_bytes = None;
                let mut chunks = None;
                let mut snapshot_offsets = None;
                let mut total_items = None;
                let mut total_memory_used = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::MaxItems => {
                            if max_items.is_some() {
                                return Err(de::Error::duplicate_field("max_items"));
                            }
                            max_items = Some(map.next_value()?);
                        }
                        Field::MaxMemoryBytes => {
                            if max_memory_bytes.is_some() {
                                return Err(de::Error::duplicate_field("max_memory_bytes"));
                            }
                            max_memory_bytes = Some(map.next_value()?);
                        }
                        Field::Chunks => {
                            if chunks.is_some() {
                                return Err(de::Error::duplicate_field("chunks"));
                            }
                            chunks = Some(map.next_value()?);
                        }
                        Field::SnapshotOffsets => {
                            if snapshot_offsets.is_some() {
                                return Err(de::Error::duplicate_field("snapshot_offsets"));
                            }
                            snapshot_offsets = Some(map.next_value()?);
                        }
                        Field::TotalItems => {
                            if total_items.is_some() {
                                return Err(de::Error::duplicate_field("total_items"));
                            }
                            total_items = Some(map.next_value()?);
                        }
                        Field::TotalMemoryUsed => {
                            if total_memory_used.is_some() {
                                return Err(de::Error::duplicate_field("total_memory_used"));
                            }
                            total_memory_used = Some(map.next_value()?);
                        }
                    }
                }

                let max_items = max_items.ok_or_else(|| de::Error::missing_field("max_items"))?;
                let max_memory_bytes =
                    max_memory_bytes.ok_or_else(|| de::Error::missing_field("max_memory_bytes"))?;
                let chunks = chunks.ok_or_else(|| de::Error::missing_field("chunks"))?;
                let snapshot_offsets =
                    snapshot_offsets.ok_or_else(|| de::Error::missing_field("snapshot_offsets"))?;
                let total_items =
                    total_items.ok_or_else(|| de::Error::missing_field("total_items"))?;
                let total_memory_used = total_memory_used
                    .ok_or_else(|| de::Error::missing_field("total_memory_used"))?;

                Ok(Arena {
                    max_items,
                    max_memory_bytes,
                    chunks,
                    snapshot_offsets,
                    total_items,
                    total_memory_used,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "max_items",
            "max_memory_bytes",
            "chunks",
            "snapshot_offsets",
            "total_items",
            "total_memory_used",
        ];

        deserializer.deserialize_struct("Arena", FIELDS, ArenaVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::println;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn arena_is_send_sync_when_t_is() {
        // Phase-3 unlock item ④ acceptance: with interior mutability gone the
        // arena (and therefore stores embedding it) can cross thread bounds.
        assert_send_sync::<Arena<i32>>();
    }

    #[test]
    fn test_arena_allocation_and_snapshots() {
        let mut arena = Arena::new(4, 1000300, 1024 * 1024 * 1024);

        // Allocate some data into the arena
        arena.alloc(42).unwrap();
        arena.alloc(100).unwrap();

        // Take a snapshot
        arena.snapshot();

        println!(
            "Arena: {} {}, {}",
            arena.total_chunks(),
            arena.total_items(),
            arena.total_memory_usage()
        );

        // Allocate more data
        arena.alloc(200).unwrap();

        // Take another snapshot
        arena.snapshot();

        println!(
            "Arena: {} {}, {}",
            arena.total_chunks(),
            arena.total_items(),
            arena.total_memory_usage()
        );

        for i in 0..100 {
            arena.alloc(i).unwrap();
        }

        arena.snapshot();

        println!(
            "Arena: {} {}, {}",
            arena.total_chunks(),
            arena.total_items(),
            arena.total_memory_usage()
        );

        let snapshot_data3_len = arena.get_snapshot(2).len();

        assert_eq!(arena.total_items(), 103);

        // Add 1M objects after snapshot, to check if any re-allocations affect the snapshot data.
        // (Snapshot references now carry real lifetimes — they cannot be held
        // across the appends below, so the snapshot content is re-fetched and
        // compared against the expected values afterwards.)
        for i in 0..1_000_000 {
            arena.must_alloc(i);
        }

        println!(
            "Arena: {} {}, {}",
            arena.total_chunks(),
            arena.total_items(),
            arena.total_memory_usage()
        );

        assert_eq!(arena.total_chunks(), 18);
        assert_eq!(arena.total_items(), 1_000_103);

        // Validate snapshot data after the further allocations
        let snapshot_data1: Vec<i32> = arena.get_snapshot(0).into_iter().copied().collect();
        let snapshot_data2: Vec<i32> = arena.get_snapshot(1).into_iter().copied().collect();
        let snapshot_data3: Vec<i32> = arena.get_snapshot(2).into_iter().copied().collect();

        assert_eq!(snapshot_data1, vec![42, 100]);
        assert_eq!(snapshot_data2, vec![42, 100, 200]);
        assert_eq!(snapshot_data3.len(), snapshot_data3_len);
        assert_eq!(snapshot_data3[0..3], [42, 100, 200]);

        for i in 0..100 {
            assert_eq!(snapshot_data3[3 + i], i as i32);
        }

        // Verify snapshots again (stable across repeated reads)
        let snapshot_data1_again: Vec<i32> = arena.get_snapshot(0).into_iter().copied().collect();
        let snapshot_data2_again: Vec<i32> = arena.get_snapshot(1).into_iter().copied().collect();
        let snapshot_data3_again: Vec<i32> = arena.get_snapshot(2).into_iter().copied().collect();

        assert_eq!(snapshot_data1, snapshot_data1_again);
        assert_eq!(snapshot_data2, snapshot_data2_again);
        assert_eq!(snapshot_data3, snapshot_data3_again);

        arena.reset();
        assert_eq!(arena.total_chunks(), 1);
        assert_eq!(arena.total_items(), 0);
        assert_eq!(arena.total_memory_usage(), 0);
    }

    #[test]
    fn test_arena_iterator() {
        let mut arena = Arena::new(4, 1000, 1024 * 1024 * 1024);

        for i in 0..1000 {
            arena.alloc(i).unwrap();
        }

        let mut iter = arena.iter();
        for i in 0..1000 {
            assert_eq!(iter.next(), Some(&i));
        }
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_arena_serialize_deserialize() {
        let mut arena = Arena::new(4, 1000, 1024 * 1024 * 1024);

        for i in 0..1000 {
            arena.alloc(i).unwrap();
        }

        // Serialize the arena
        let serialized = serde_json::to_vec(&arena).unwrap();

        // Deserialize it back to an Arena
        let deserialized: Arena<i32> = serde_json::from_slice(&serialized).unwrap();

        // Verify that the deserialized arena has the same elements
        let mut iter = deserialized.iter();
        for i in 0..1000 {
            assert_eq!(iter.next(), Some(&i));
        }
        assert_eq!(iter.next(), None);

        // Additionally, verify that other fields are equal
        assert_eq!(arena.max_items, deserialized.max_items);
        assert_eq!(arena.max_memory_bytes, deserialized.max_memory_bytes);
        assert_eq!(arena.total_items, deserialized.total_items);
        assert_eq!(arena.total_memory_used, deserialized.total_memory_used);
    }

    #[test]
    fn test_alloc_and_retrieve() {
        let mut arena = Arena::new(4, 100, 1024 * 1024);

        let a: String = "Hello, World!".into();

        // Test advanced_alloc: returns placement indices plus the element
        // reference; the exclusive borrow ends at its last use, so the next
        // allocation below compiles (the pre-Phase-3 variant could hold the
        // `&mut` across further allocations only because it was derived
        // unsafely from `&self`).
        let (c0, i0, elem_ref) = arena.advanced_alloc(a).expect("Allocation failed");
        assert_eq!(elem_ref, &"Hello, World!");
        println!("{:?},{:?}", c0, i0);

        let b = "Hello, again!".into();
        let (c1, i1, elem_ref1) = arena.advanced_alloc(b).expect("Advanced allocation failed");
        assert_eq!(elem_ref1, &"Hello, again!");
        println!("{:?},{:?}", c1, i1);

        // Retrieve the elements using the indices and verify them
        let element = arena.get(c1, i1).expect("Element not found");
        assert_eq!(element, &"Hello, again!");
        println!("{:?}", element);

        //update a
        arena
            .get_mut(c0, i0)
            .expect("Element not found")
            .push_str("!!!");
        let element = arena.get(c0, i0).expect("Element not found");
        println!("{:?}", element);
        assert_eq!(element, &"Hello, World!!!!");

        //update b
        arena
            .get_mut(c1, i1)
            .expect("Element not found")
            .push_str("???");
        let element = arena.get(c1, i1).expect("Element not found");
        println!("{:?}", element);
        assert_eq!(element, &"Hello, again!???");
    }
}
