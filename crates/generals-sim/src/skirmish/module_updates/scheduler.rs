//! Concrete native module dispatcher and source heap; callback bodies are caller-owned.

const FOREVER: u32 = 0x3fff_ffff;

struct Update {
    priority: u32,
    phase: u32,
    index: Option<usize>,
}

struct SleepyHeap {
    updates: Vec<Update>,
    heap: Vec<usize>,
    current: Option<usize>,
}

impl SleepyHeap {
    const fn new() -> Self {
        Self {
            updates: Vec::new(),
            heap: Vec::new(),
            current: None,
        }
    }

    /// Allocate the stable module identity before its constructor wakes it.
    fn create(&mut self, phase: u32) -> usize {
        assert!(phase < 4);
        let id = self.updates.len();
        self.updates.push(Update {
            priority: 0,
            phase,
            index: None,
        });
        id
    }

    /// `GameLogic` registers updates in the object's authored module order.
    fn register(&mut self, id: usize, now: u32) {
        assert!(self.updates[id].index.is_none());
        if self.updates[id].priority >> 2 == 0 {
            self.set_frame(id, now.max(1));
        }
        self.updates[id].index = Some(self.heap.len());
        self.heap.push(id);
        self.rebalance_parent(self.heap.len() - 1);
    }

    /// `UpdateModule::setWakeFrame`, including calls made before registration.
    fn set_wake(&mut self, id: usize, now: u32, delay: u32) {
        let when = now.wrapping_add(delay);
        if self.current == Some(id) {
            return;
        }
        let next = self.updates[id].priority >> 2;
        if when == next || (now > 0 && next == now && when == now.wrapping_add(1)) {
            return;
        }
        self.set_frame(id, when);
        if let Some(index) = self.updates[id].index {
            self.rebalance(index);
        }
    }

    /// `UpdateModule::getWakeFrame`: a due or overdue module reports NONE=1.
    fn remaining_sleep(&self, id: usize, now: u32) -> u32 {
        let next = self.updates[id].priority >> 2;
        if next > now { next - now } else { 1 }
    }

    fn next_due(&self, now: u32) -> Option<usize> {
        self.heap
            .first()
            .copied()
            .filter(|&id| self.updates[id].priority >> 2 <= now)
    }

    /// Disabled callbacks skip this step, then finish with delay one.
    fn begin_callback(&mut self, id: usize) {
        assert!(self.current.is_none());
        assert_eq!(self.heap.first(), Some(&id));
        self.current = Some(id);
    }

    /// Native callbacks remain in the heap while executing.
    fn finish_callback(&mut self, id: usize, now: u32, delay: i32) {
        assert!(self.current.is_none() || self.current == Some(id));
        self.current = None;
        let delay = u32::try_from(delay.max(1)).expect("positive update delay");
        self.set_frame(id, now.wrapping_add(delay));
        // Native GameLogic rebalances index zero, not the callback's saved index.
        self.rebalance(0);
    }

    fn erase(&mut self, id: usize) {
        let index = self.updates[id].index.take().expect("registered update");
        let last = self.heap.pop().expect("nonempty update heap");
        if index < self.heap.len() {
            self.heap[index] = last;
            self.updates[last].index = Some(index);
            self.rebalance(index);
        }
    }

    fn reset(&mut self) {
        for id in self.heap.drain(..) {
            self.updates[id].index = None;
        }
        self.current = None;
    }

    fn set_frame(&mut self, id: usize, frame: u32) {
        let update = &mut self.updates[id];
        update.priority = (frame.min(FOREVER) << 2) | update.phase;
    }

    fn lower_priority(&self, left: usize, right: usize) -> bool {
        self.updates[self.heap[left]].priority > self.updates[self.heap[right]].priority
    }

    fn swap(&mut self, left: usize, right: usize) {
        self.heap.swap(left, right);
        self.updates[self.heap[left]].index = Some(left);
        self.updates[self.heap[right]].index = Some(right);
    }

    fn rebalance_parent(&mut self, mut index: usize) -> usize {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.lower_priority(parent, index) {
                break;
            }
            self.swap(parent, index);
            index = parent;
        }
        index
    }

    fn rebalance_child(&mut self, mut index: usize) {
        let mut child = index * 2 + 1;
        while child < self.heap.len() {
            if child + 1 < self.heap.len() && self.lower_priority(child, child + 1) {
                child += 1;
            }
            if !self.lower_priority(index, child) {
                break;
            }
            self.swap(index, child);
            index = child;
            child = index * 2 + 1;
        }
    }

    fn rebalance(&mut self, index: usize) {
        let index = self.rebalance_parent(index);
        self.rebalance_child(index);
    }
}

struct Module {
    owner: usize,
    disabled_types: u32,
}

/// One scheduler for every update-capable module, including object helpers.
/// Runtime descriptors retain the callback kind and instance ordinal by this ID.
pub(in crate::skirmish) struct Scheduler {
    heap: SleepyHeap,
    modules: Vec<Module>,
    destruction: Vec<usize>,
    destruction_cursor: usize,
}

impl Scheduler {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            heap: SleepyHeap::new(),
            modules: Vec::new(),
            destruction: Vec::new(),
            destruction_cursor: 0,
        }
    }

    /// Allocate before the real constructor invokes any setWakeFrame operation.
    /// Phase and accepted disabled bits come from that concrete source module.
    pub(in crate::skirmish) fn create(
        &mut self,
        owner: usize,
        phase: u32,
        disabled_types: u32,
    ) -> usize {
        assert_eq!(disabled_types & !0x1fff, 0);
        let id = self.heap.create(phase);
        assert_eq!(id, self.modules.len());
        self.modules.push(Module {
            owner,
            disabled_types,
        });
        id
    }

    /// `Object::init` supplies helpers followed by update-capable authored modules.
    pub(in crate::skirmish) fn register_object(
        &mut self,
        owner: usize,
        modules: &[usize],
        now: u32,
    ) {
        for &id in modules {
            assert_eq!(self.modules[id].owner, owner);
            self.heap.register(id, now);
        }
    }

    pub(in crate::skirmish) fn set_wake(&mut self, id: usize, now: u32, delay: u32) {
        self.heap.set_wake(id, now, delay);
    }

    pub(in crate::skirmish) fn remaining_sleep(&self, id: usize, now: u32) -> u32 {
        self.heap.remaining_sleep(id, now)
    }

    pub(in crate::skirmish) fn next_due(&self, now: u32) -> Option<usize> {
        assert!(self.heap.current.is_none());
        self.heap.next_due(now)
    }

    pub(in crate::skirmish) fn owner(&self, id: usize) -> usize {
        self.modules[id].owner
    }

    /// Fixed CRC=0 exclusive disabled mask. False consumes this due entry with
    /// NONE=1 without running the module callback or setting the current pointer.
    pub(in crate::skirmish) fn begin_due(&mut self, id: usize, now: u32, disabled: u32) -> bool {
        assert_eq!(self.next_due(now), Some(id));
        assert_eq!(disabled & !0x1fff, 0);
        if disabled & !self.modules[id].disabled_types == 0 {
            self.heap.begin_callback(id);
            true
        } else {
            self.heap.finish_callback(id, now, 1);
            false
        }
    }

    pub(in crate::skirmish) fn finish(&mut self, id: usize, now: u32, sleep: i32) {
        assert_eq!(self.heap.current, Some(id));
        self.heap.finish_callback(id, now, sleep);
    }

    /// Call exactly where native destroyObject appends, after its destroyed flag
    /// and AI cancellation. Caller owns onDestroy callbacks and duplicate guard.
    /// Pending destruction does not suppress other same-frame module callbacks.
    pub(in crate::skirmish) fn queue_destroy(&mut self, owner: usize) {
        self.destruction.push(owner);
    }

    /// End-frame processDestroyList: unregister before the real object destructor.
    /// The caller may append cascading destruction during that destructor before
    /// requesting the next object; newly registered updates wait for the next
    /// ordinary due-module loop, even when already due.
    pub(in crate::skirmish) fn next_destruction(&mut self) -> Option<usize> {
        assert!(self.heap.current.is_none());
        let Some(&owner) = self.destruction.get(self.destruction_cursor) else {
            self.destruction.clear();
            self.destruction_cursor = 0;
            return None;
        };
        self.destruction_cursor += 1;
        // Native processDestroyList scans current heap order, then erases backward.
        let registered: Vec<_> = self
            .heap
            .heap
            .iter()
            .copied()
            .filter(|&id| self.modules[id].owner == owner)
            .take(256)
            .collect();
        for id in registered.into_iter().rev() {
            self.heap.erase(id);
        }
        Some(owner)
    }

    /// Call after native destroyAllObjectsImmediate has completed domain teardown.
    /// Old module handles must be discarded with their runtime objects.
    pub(in crate::skirmish) fn reset(&mut self) {
        assert!(self.heap.current.is_none());
        self.heap.reset();
        *self = Self::new();
    }
}
