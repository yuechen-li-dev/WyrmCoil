/// Exit order is leaf to root. The caller owns the cleanup and return policy.
pub(super) fn Unwind<F>(stack: &mut Vec<F>, keep: usize, mut exit: impl FnMut(F)) {
    while stack.len() > keep {
        exit(stack.pop().unwrap());
    }
}
