// Adapted from CPython 3.12.10 Objects/listobject.c at 0cc81280367df838c4b199f8f0378837165071c2.
// The safe Rust port sorts indices and retains the comparison order, run policy and galloping.
// See licenses/CPython-3.12.10-LICENSE and licenses/CPython-3.12.10-NOTICE.md.
use super::errors::{ValidationFailure, ValidationResult};

const MIN_GALLOP: usize = 7;

pub(crate) fn identity_indices(length: usize) -> ValidationResult<Vec<usize>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| ValidationFailure::python("MemoryError", ""))?;
    values.extend(0..length);
    Ok(values)
}

#[derive(Clone, Copy)]
struct Run {
    start: usize,
    length: usize,
    power: u32,
}

enum Finish {
    Done,
    Single,
}

struct Sorter<'a, F> {
    values: &'a mut [usize],
    less: F,
    runs: Vec<Run>,
    min_gallop: usize,
}

fn minrun(mut length: usize) -> usize {
    let mut remainder = 0;
    while length >= 64 {
        remainder |= length & 1;
        length >>= 1;
    }
    length + remainder
}

fn power(start: usize, left: usize, right: usize, length: usize) -> u32 {
    let mut a = 2 * start as u128 + left as u128;
    let mut b = a + left as u128 + right as u128;
    let length = length as u128;
    let mut result = 0;
    loop {
        result += 1;
        if a >= length {
            a -= length;
            b -= length;
        } else if b >= length {
            return result;
        }
        a <<= 1;
        b <<= 1;
    }
}

fn gallop_left<F>(
    key: usize,
    values: &[usize],
    hint: usize,
    less: &mut F,
) -> ValidationResult<usize>
where
    F: FnMut(usize, usize) -> ValidationResult<bool>,
{
    let mut previous = 0;
    let mut offset = 1_usize;
    let (mut low, mut high);
    if less(values[hint], key)? {
        let maximum = values.len() - hint;
        while offset < maximum {
            if !less(values[hint + offset], key)? {
                break;
            }
            previous = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        low = previous + hint + 1;
        high = offset + hint;
    } else {
        let maximum = hint + 1;
        while offset < maximum {
            if less(values[hint - offset], key)? {
                break;
            }
            previous = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        low = hint + 1 - offset;
        high = hint - previous;
    }
    while low < high {
        let middle = low + (high - low) / 2;
        if less(values[middle], key)? {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    Ok(high)
}

fn gallop_right<F>(
    key: usize,
    values: &[usize],
    hint: usize,
    less: &mut F,
) -> ValidationResult<usize>
where
    F: FnMut(usize, usize) -> ValidationResult<bool>,
{
    let mut previous = 0;
    let mut offset = 1_usize;
    let (mut low, mut high);
    if less(key, values[hint])? {
        let maximum = hint + 1;
        while offset < maximum {
            if !less(key, values[hint - offset])? {
                break;
            }
            previous = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        low = hint + 1 - offset;
        high = hint - previous;
    } else {
        let maximum = values.len() - hint;
        while offset < maximum {
            if less(key, values[hint + offset])? {
                break;
            }
            previous = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        low = previous + hint + 1;
        high = offset + hint;
    }
    while low < high {
        let middle = low + (high - low) / 2;
        if less(key, values[middle])? {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(high)
}

impl<F> Sorter<'_, F>
where
    F: FnMut(usize, usize) -> ValidationResult<bool>,
{
    fn count_run(&mut self, start: usize) -> ValidationResult<(usize, bool)> {
        if start + 1 == self.values.len() {
            return Ok((1, false));
        }
        let descending = (self.less)(self.values[start + 1], self.values[start])?;
        let mut length = 2;
        while start + length < self.values.len() {
            if (self.less)(self.values[start + length], self.values[start + length - 1])?
                != descending
            {
                break;
            }
            length += 1;
        }
        Ok((length, descending))
    }

    fn binary_sort(&mut self, start: usize, length: usize, sorted: usize) -> ValidationResult<()> {
        for index in start + sorted.max(1)..start + length {
            let pivot = self.values[index];
            let mut low = start;
            let mut high = index;
            while low < high {
                let middle = low + (high - low) / 2;
                if (self.less)(pivot, self.values[middle])? {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            self.values.copy_within(low..index, low + 1);
            self.values[low] = pivot;
        }
        Ok(())
    }

    fn merge_low(
        &mut self,
        left: usize,
        mut left_count: usize,
        right: usize,
        mut right_count: usize,
    ) -> ValidationResult<()> {
        let mut temporary = Vec::new();
        temporary
            .try_reserve_exact(left_count)
            .map_err(|_| ValidationFailure::python("MemoryError", ""))?;
        temporary.extend_from_slice(&self.values[left..left + left_count]);
        let mut a = 0;
        let mut b = right;
        let mut destination = left;
        self.values[destination] = self.values[b];
        destination += 1;
        b += 1;
        right_count -= 1;
        let result = (|| -> ValidationResult<Finish> {
            if right_count == 0 {
                return Ok(Finish::Done);
            }
            if left_count == 1 {
                return Ok(Finish::Single);
            }
            let mut minimum = self.min_gallop;
            loop {
                let mut a_count = 0;
                let mut b_count = 0;
                loop {
                    if (self.less)(self.values[b], temporary[a])? {
                        self.values[destination] = self.values[b];
                        destination += 1;
                        b += 1;
                        b_count += 1;
                        a_count = 0;
                        right_count -= 1;
                        if right_count == 0 {
                            return Ok(Finish::Done);
                        }
                        if b_count >= minimum {
                            break;
                        }
                    } else {
                        self.values[destination] = temporary[a];
                        destination += 1;
                        a += 1;
                        a_count += 1;
                        b_count = 0;
                        left_count -= 1;
                        if left_count == 1 {
                            return Ok(Finish::Single);
                        }
                        if a_count >= minimum {
                            break;
                        }
                    }
                }
                minimum += 1;
                loop {
                    minimum -= usize::from(minimum > 1);
                    self.min_gallop = minimum;
                    a_count = gallop_right(
                        self.values[b],
                        &temporary[a..a + left_count],
                        0,
                        &mut self.less,
                    )?;
                    if a_count != 0 {
                        self.values[destination..destination + a_count]
                            .copy_from_slice(&temporary[a..a + a_count]);
                        destination += a_count;
                        a += a_count;
                        left_count -= a_count;
                        if left_count == 1 {
                            return Ok(Finish::Single);
                        }
                        if left_count == 0 {
                            return Ok(Finish::Done);
                        }
                    }
                    self.values[destination] = self.values[b];
                    destination += 1;
                    b += 1;
                    right_count -= 1;
                    if right_count == 0 {
                        return Ok(Finish::Done);
                    }
                    b_count = gallop_left(
                        temporary[a],
                        &self.values[b..b + right_count],
                        0,
                        &mut self.less,
                    )?;
                    if b_count != 0 {
                        self.values.copy_within(b..b + b_count, destination);
                        destination += b_count;
                        b += b_count;
                        right_count -= b_count;
                        if right_count == 0 {
                            return Ok(Finish::Done);
                        }
                    }
                    self.values[destination] = temporary[a];
                    destination += 1;
                    a += 1;
                    left_count -= 1;
                    if left_count == 1 {
                        return Ok(Finish::Single);
                    }
                    if a_count < MIN_GALLOP && b_count < MIN_GALLOP {
                        break;
                    }
                }
                minimum += 1;
                self.min_gallop = minimum;
            }
        })();
        match result {
            Ok(Finish::Single) => {
                self.values.copy_within(b..b + right_count, destination);
                self.values[destination + right_count] = temporary[a];
                Ok(())
            }
            result => {
                if left_count != 0 {
                    self.values[destination..destination + left_count]
                        .copy_from_slice(&temporary[a..a + left_count]);
                }
                result.map(|_| ())
            }
        }
    }

    fn merge_high(
        &mut self,
        left: usize,
        mut left_count: usize,
        right: usize,
        mut right_count: usize,
    ) -> ValidationResult<()> {
        let mut temporary = Vec::new();
        temporary
            .try_reserve_exact(right_count)
            .map_err(|_| ValidationFailure::python("MemoryError", ""))?;
        temporary.extend_from_slice(&self.values[right..right + right_count]);
        let mut a_end = left + left_count;
        let mut b_end = right_count;
        let mut destination_end = right + right_count;
        a_end -= 1;
        destination_end -= 1;
        self.values[destination_end] = self.values[a_end];
        left_count -= 1;
        let result = (|| -> ValidationResult<Finish> {
            if left_count == 0 {
                return Ok(Finish::Done);
            }
            if right_count == 1 {
                return Ok(Finish::Single);
            }
            let mut minimum = self.min_gallop;
            loop {
                let mut a_count = 0;
                let mut b_count = 0;
                loop {
                    if (self.less)(temporary[b_end - 1], self.values[a_end - 1])? {
                        destination_end -= 1;
                        a_end -= 1;
                        self.values[destination_end] = self.values[a_end];
                        a_count += 1;
                        b_count = 0;
                        left_count -= 1;
                        if left_count == 0 {
                            return Ok(Finish::Done);
                        }
                        if a_count >= minimum {
                            break;
                        }
                    } else {
                        destination_end -= 1;
                        b_end -= 1;
                        self.values[destination_end] = temporary[b_end];
                        b_count += 1;
                        a_count = 0;
                        right_count -= 1;
                        if right_count == 1 {
                            return Ok(Finish::Single);
                        }
                        if b_count >= minimum {
                            break;
                        }
                    }
                }
                minimum += 1;
                loop {
                    minimum -= usize::from(minimum > 1);
                    self.min_gallop = minimum;
                    a_count = left_count
                        - gallop_right(
                            temporary[b_end - 1],
                            &self.values[left..left + left_count],
                            left_count - 1,
                            &mut self.less,
                        )?;
                    if a_count != 0 {
                        self.values
                            .copy_within(a_end - a_count..a_end, destination_end - a_count);
                        destination_end -= a_count;
                        a_end -= a_count;
                        left_count -= a_count;
                        if left_count == 0 {
                            return Ok(Finish::Done);
                        }
                    }
                    destination_end -= 1;
                    b_end -= 1;
                    self.values[destination_end] = temporary[b_end];
                    right_count -= 1;
                    if right_count == 1 {
                        return Ok(Finish::Single);
                    }
                    b_count = right_count
                        - gallop_left(
                            self.values[a_end - 1],
                            &temporary[..right_count],
                            right_count - 1,
                            &mut self.less,
                        )?;
                    if b_count != 0 {
                        self.values[destination_end - b_count..destination_end]
                            .copy_from_slice(&temporary[b_end - b_count..b_end]);
                        destination_end -= b_count;
                        b_end -= b_count;
                        right_count -= b_count;
                        if right_count == 1 {
                            return Ok(Finish::Single);
                        }
                        if right_count == 0 {
                            return Ok(Finish::Done);
                        }
                    }
                    destination_end -= 1;
                    a_end -= 1;
                    self.values[destination_end] = self.values[a_end];
                    left_count -= 1;
                    if left_count == 0 {
                        return Ok(Finish::Done);
                    }
                    if a_count < MIN_GALLOP && b_count < MIN_GALLOP {
                        break;
                    }
                }
                minimum += 1;
                self.min_gallop = minimum;
            }
        })();
        match result {
            Ok(Finish::Single) => {
                self.values
                    .copy_within(a_end - left_count..a_end, destination_end - left_count);
                self.values[destination_end - left_count - 1] = temporary[b_end - 1];
                Ok(())
            }
            result => {
                if right_count != 0 {
                    self.values[destination_end - right_count..destination_end]
                        .copy_from_slice(&temporary[..right_count]);
                }
                result.map(|_| ())
            }
        }
    }

    fn merge_at(&mut self, index: usize) -> ValidationResult<()> {
        let mut left = self.runs[index].start;
        let mut left_count = self.runs[index].length;
        let right = self.runs[index + 1].start;
        let mut right_count = self.runs[index + 1].length;
        self.runs[index].length = left_count + right_count;
        self.runs.remove(index + 1);
        let skip = gallop_right(
            self.values[right],
            &self.values[left..left + left_count],
            0,
            &mut self.less,
        )?;
        left += skip;
        left_count -= skip;
        if left_count == 0 {
            return Ok(());
        }
        right_count = gallop_left(
            self.values[left + left_count - 1],
            &self.values[right..right + right_count],
            right_count - 1,
            &mut self.less,
        )?;
        if right_count == 0 {
            return Ok(());
        }
        if left_count <= right_count {
            self.merge_low(left, left_count, right, right_count)
        } else {
            self.merge_high(left, left_count, right, right_count)
        }
    }

    fn found_run(&mut self, length: usize) -> ValidationResult<()> {
        if let Some(previous) = self.runs.last().copied() {
            let power = power(previous.start, previous.length, length, self.values.len());
            while self.runs.len() > 1 && self.runs[self.runs.len() - 2].power > power {
                self.merge_at(self.runs.len() - 2)?;
            }
            self.runs.last_mut().unwrap().power = power;
        }
        Ok(())
    }

    fn sort(&mut self) -> ValidationResult<()> {
        let minimum = minrun(self.values.len());
        let mut start = 0;
        while start < self.values.len() {
            let (mut length, descending) = self.count_run(start)?;
            if descending {
                self.values[start..start + length].reverse();
            }
            if length < minimum {
                let forced = minimum.min(self.values.len() - start);
                self.binary_sort(start, forced, length)?;
                length = forced;
            }
            self.found_run(length)?;
            self.runs.push(Run {
                start,
                length,
                power: 0,
            });
            start += length;
        }
        while self.runs.len() > 1 {
            let mut index = self.runs.len() - 2;
            if index > 0 && self.runs[index - 1].length < self.runs[index + 1].length {
                index -= 1;
            }
            self.merge_at(index)?;
        }
        Ok(())
    }
}

pub(crate) fn sort_indices(
    values: &mut [usize],
    less: impl FnMut(usize, usize) -> ValidationResult<bool>,
) -> ValidationResult<()> {
    if values.len() < 2 {
        return Ok(());
    }
    let mut runs = Vec::new();
    runs.try_reserve_exact(usize::BITS as usize)
        .map_err(|_| ValidationFailure::python("MemoryError", ""))?;
    Sorter {
        values,
        less,
        runs,
        min_gallop: MIN_GALLOP,
    }
    .sort()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::python_operations::sort_by_less;

    #[test]
    fn stable_sort_covers_run_boundaries_and_both_merge_directions() {
        for length in [
            0, 1, 2, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257, 511, 1024, 4096,
        ] {
            for pattern in 0..6 {
                let mut seed = 0x13d2_90a7_u64;
                let mut values = (0..length)
                    .map(|index| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        let key = match pattern {
                            0 => (seed >> 32) as i64 % 37,
                            1 => (length - index) as i64,
                            2 => index as i64,
                            3 => (index % 73) as i64,
                            4 => {
                                if index < length / 3 {
                                    (index * 2) as i64
                                } else {
                                    ((index - length / 3) * 2 + 1) as i64
                                }
                            }
                            _ => ((length - index) / 5) as i64,
                        };
                        (key, index)
                    })
                    .collect::<Vec<_>>();
                let mut expected = values.clone();
                expected.sort_by_key(|value| value.0);
                sort_by_less(&mut values, |left, right| Ok(left.0 < right.0)).unwrap();
                assert_eq!(values, expected, "length={length}, pattern={pattern}");
            }
        }
    }

    #[test]
    fn ownership_does_not_require_cloning_values() {
        #[derive(Debug, PartialEq, Eq)]
        struct Owned {
            key: i32,
            payload: Box<str>,
        }
        let mut values = (0..100)
            .rev()
            .map(|key| Owned {
                key,
                payload: format!("item-{key}").into_boxed_str(),
            })
            .collect::<Vec<_>>();
        sort_by_less(&mut values, |a, b| Ok(a.key < b.key)).unwrap();
        for (index, value) in values.iter().enumerate() {
            assert_eq!(value.key, index as i32);
            assert_eq!(&*value.payload, format!("item-{index}"));
        }
    }

    #[test]
    fn comparison_errors_keep_a_permutation_during_run_and_merge_phases() {
        for stop in (1..2048).step_by(17) {
            let mut values = (0..1024)
                .map(|index| ((index * 613) % 1024, index))
                .collect::<Vec<_>>();
            let mut calls = 0;
            let result = sort_by_less(&mut values, |left, right| {
                calls += 1;
                if calls == stop {
                    return Err(ValidationFailure::python("TypeError", "comparison failed"));
                }
                Ok(left.0 < right.0)
            });
            assert!(
                matches!(result,Err(ValidationFailure::Python{kind:"TypeError",message})if message.equals("comparison failed"))
            );
            let mut identifiers = values.iter().map(|value| value.1).collect::<Vec<_>>();
            identifiers.sort_unstable();
            assert_eq!(identifiers, (0..1024).collect::<Vec<_>>(), "stop={stop}");
        }
    }

    #[test]
    fn comparison_order_matches_cpython_run_detection_and_binary_insertion() {
        let mut values = vec![3, 1, 2];
        let mut comparisons = Vec::new();
        sort_by_less(&mut values, |left, right| {
            comparisons.push((*left, *right));
            Ok(left < right)
        })
        .unwrap();
        assert_eq!(comparisons, vec![(1, 3), (2, 1), (2, 3), (2, 1)]);
        assert_eq!(values, vec![1, 2, 3]);
    }

    #[test]
    fn leading_and_trailing_nan_keep_the_source_run_and_trim_behavior() {
        for length in [64, 65, 127, 128, 129, 511] {
            let mut leading = vec![f64::NAN];
            leading.extend((1..length).rev().map(|value| value as f64));
            sort_by_less(&mut leading, |left, right| Ok(left < right)).unwrap();
            assert!(leading[0].is_nan());
            assert_eq!(
                &leading[1..],
                &(1..length).map(|value| value as f64).collect::<Vec<_>>()
            );
            let mut trailing = (1..length)
                .rev()
                .map(|value| value as f64)
                .collect::<Vec<_>>();
            trailing.push(f64::NAN);
            sort_by_less(&mut trailing, |left, right| Ok(left < right)).unwrap();
            assert!(trailing[length - 1].is_nan());
            assert_eq!(
                &trailing[..length - 1],
                &(1..length).map(|value| value as f64).collect::<Vec<_>>()
            );
        }
    }
}
