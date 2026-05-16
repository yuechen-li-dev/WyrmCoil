#![allow(non_snake_case)]

pub fn DenseAliveCount(alive: &[bool]) -> usize {
    alive.iter().filter(|it| **it).count()
}

pub fn DenseAliveIndices(alive: &[bool]) -> Vec<usize> {
    let mut indices = Vec::new();
    for (index, is_alive) in alive.iter().enumerate() {
        if *is_alive {
            indices.push(index);
        }
    }
    indices
}

pub fn DenseLaneSafeLen(lengths: &[usize]) -> usize {
    let mut min_len = usize::MAX;
    for length in lengths {
        min_len = min_len.min(*length);
    }
    if min_len == usize::MAX { 0 } else { min_len }
}
