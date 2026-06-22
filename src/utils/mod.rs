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

pub mod rand;

pub mod uuid;

pub mod json;
mod maplit;
pub mod strings;

pub mod sequencer {
    use serde::Deserialize;
    use serde::Serialize;

    #[derive(Clone, Debug, Default, Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    /// A `Sequencer` that has `Deserialize` and `Serialize` implemented.
    pub struct Sequencer {
        offset: u32,
        step: u32,

        max: u32,
    }

    impl Sequencer {
        /// Create a new [`Sequencer`].
        pub fn new(offset: u32, step: u32, max: u32) -> Sequencer {
            Sequencer { offset, step, max }
        }

        /// Get the current value of this [`Sequencer`].
        pub fn current(&self) -> u32 {
            self.offset
        }

        /// Get the step (stride) of this [`Sequencer`].
        pub fn step(&self) -> u32 {
            self.step
        }

        /// Get the maximum doc ID of this [`Sequencer`].
        pub fn max_id(&self) -> u32 {
            self.max
        }

        pub fn free(&self) -> u32 {
            self.max.saturating_sub(self.offset)
        }
    }

    impl Iterator for Sequencer {
        type Item = u32;

        fn next(&mut self) -> Option<Self::Item> {
            let current = self.offset;

            if current <= self.max {
                self.offset += self.step;
                Some(current)
            } else {
                None
            }
        }
    }

    #[test]
    fn check_first_value() {
        let mut sequencer = Sequencer::new(0, 1, 5);
        let first = sequencer.next().unwrap();
        assert_eq!(first, 0);
        assert_eq!(sequencer.current(), 1);
        assert_eq!(sequencer.free(), 4);

        let mut sequencer = Sequencer::new(0, 3, 5);
        let first = sequencer.next().unwrap();
        assert_eq!(first, 0);
        assert_eq!(sequencer.current(), 3);
        assert_eq!(sequencer.free(), 2);
    }

    #[test]
    fn check_last_value() {
        let sequencer = Sequencer::new(0, 1, 1000);
        let last = sequencer.last().unwrap();
        assert_eq!(last, 1000);

        let sequencer = Sequencer::new(0, 256, 1000);
        let last = sequencer.last().unwrap();
        assert_eq!(last, 768);
    }

    #[test]
    fn sequencer_works() {
        let mut sequencer = Sequencer::new(0, 100, 500);

        for i in (0..=500).step_by(100) {
            assert_eq!(sequencer.next().unwrap(), i);
            assert_eq!(sequencer.current(), i + 100);
            assert_eq!(
                sequencer.free(),
                500_u32.saturating_sub(sequencer.current())
            );
        }

        let mut sequencer = Sequencer::new(0, 256, 500);

        for i in (0..=500).step_by(256) {
            assert_eq!(sequencer.next().unwrap(), i);
            assert_eq!(sequencer.current(), i + 256);
            assert_eq!(
                sequencer.free(),
                500_u32.saturating_sub(sequencer.current())
            );
        }
    }

    #[test]
    fn check_free() {
        let mut sequencer = Sequencer::new(0, 100, 5);
        assert_eq!(sequencer.free(), 5);
        assert_eq!(sequencer.next().unwrap(), 0);
        assert_eq!(sequencer.free(), 0);
    }
}
