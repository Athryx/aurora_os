use arrayvec::ArrayVec;

use super::Box;
use crate::alloc::HeapRef;
use crate::prelude::*;

// this should be an odd number
const BTREE_ORDER: usize = 9;
const BTREE_NUM_ELEMS: usize = BTREE_ORDER - 1;
// with this depth and an order of 9, the btree can store more lements than there are bytes on most computer's memory
const BTREE_MAX_SEARCH_DEPTH: usize = 16;

struct TreeNode<K: Ord, V> {
    values: ArrayVec<(K, V), BTREE_NUM_ELEMS>,
    children: ArrayVec<Box<TreeNode<K, V>>, BTREE_ORDER>,
}

// Result of searching for a key in a TreeNode
#[derive(Debug, Clone, Copy)]
enum LocateResult {
    // Value is found in this node at the given index
    Value(usize),
    // Value might be in the subtree at this index
    SubTree(usize),
    // Value is not found
    None,
}

struct ValueCombination<K: Ord, V> {
    key: K,
    value: V,
    right_child: Box<TreeNode<K, V>>,
}

impl<K: Ord, V> TreeNode<K, V> {
    fn new() -> Self {
        TreeNode {
            values: ArrayVec::new(),
            children: ArrayVec::new(),
        }
    }

    fn is_child_node(&self) -> bool {
        self.children.is_empty()
    }

    fn is_full(&self) -> bool {
        self.values.len() == self.values.capacity()
    }

    fn is_min_size(&self) -> bool {
        // NOTE: only works when capcity is even, but it should never be configured to be odd
        self.values.len() <= self.values.capacity() / 2
    }

    fn insert_or_split_inner(
        &mut self,
        mut key: K,
        mut value: V,
        mut right_child: Option<Box<TreeNode<K, V>>>,
        allocer: &HeapRef,
    ) -> KResult<Option<ValueCombination<K, V>>> {
        match self.values.binary_search_by(|probe| probe.0.cmp(&key)) {
            Ok(_) => Ok(None),
            Err(i) => {
                if self.is_full() {
                    let mut other_node = Box::new(Self::new(), allocer.clone())?;

                    let mut i = 0;
                    while i < BTREE_NUM_ELEMS / 2 {
                        // Panic safety: we know the node has BTREE_NUM_ELEMS, and we only pop half of them
                        let current_value = self.values.pop().unwrap();
                        let current_value_child = self.children.pop().unwrap();

                        if key > current_value.0 {
                            // Panic safety: this node will have enough space for num elements / 2
                            other_node.values.insert(0, (key, value));

                            key = current_value.0;
                            value = current_value.1;

                            // don't modify children on a leaf node
                            if let Some(right_child_inner) = right_child {
                                other_node.children.insert(0, right_child_inner);
                                right_child = Some(current_value_child);
                            }
                        } else {
                            // Panic safety: this node will have enough space for num elements / 2
                            other_node.values.insert(0, current_value);
                            other_node.children.insert(0, current_value_child);
                        }
                        i += 1;
                    }

                    if let Some(right_child) = right_child {
                        other_node.children.insert(0, right_child);
                    }

                    Ok(Some(ValueCombination {
                        key,
                        value,
                        right_child: other_node,
                    }))
                } else {
                    // Panic safety: the node is not full
                    self.values.insert(i, (key, value));
                    if let Some(right_child) = right_child {
                        self.children.insert(i + 1, right_child);
                    }

                    Ok(None)
                }
            },
        }
    }

    // doesn't do anything if the value is already present in the node
    // inserts the value and its child if the node has enough space
    // splits the node and returns a value combination that can be
    // inserted into its parent to make the tree right if the node is full
    fn insert_value_or_split(
        &mut self,
        value: ValueCombination<K, V>,
        allocer: &HeapRef,
    ) -> KResult<Option<ValueCombination<K, V>>> {
        let ValueCombination {
            key,
            value,
            right_child,
        } = value;
        self.insert_or_split_inner(key, value, Some(right_child), allocer)
    }

    fn insert_leaf_value_or_split(
        &mut self,
        key: K,
        value: V,
        allocer: &HeapRef,
    ) -> KResult<Option<ValueCombination<K, V>>> {
        self.insert_or_split_inner(key, value, None, allocer)
    }

    fn locate_element(&self, key: &K) -> LocateResult {
        match self.values.binary_search_by(|probe| probe.0.cmp(key)) {
            Ok(i) => LocateResult::Value(i),
            Err(i) => {
                if self.is_child_node() {
                    LocateResult::None
                } else {
                    LocateResult::SubTree(i)
                }
            },
        }
    }
}

struct NodeVisitor<'a, K: Ord, V> {
    tree: &'a mut BTreeMap<K, V>,
    nodes: ArrayVec<*mut TreeNode<K, V>, BTREE_MAX_SEARCH_DEPTH>,
}

impl<'a, K: Ord, V> NodeVisitor<'a, K, V> {
    fn new(tree: &'a mut BTreeMap<K, V>) -> Self {
        NodeVisitor {
            tree,
            nodes: ArrayVec::new(),
        }
    }

    fn visit_nodes<T, F: FnMut(&mut TreeNode<K, V>) -> Result<usize, T>>(&mut self, mut f: F) -> Option<T> {
        let mut current_node = self.tree.root.as_mut().map(|e| (&mut **e) as *mut _);

        while let Some(node_ptr) = current_node {
            // Safety: only 1 mutable node will exist into the tree at a time,
            // since only 1 is created per loop iteration and we have mutable owne
            // FIXME: use as_mut().unwrap() to check nullptr
            let node = unsafe { &mut *node_ptr };
            match f(node) {
                Ok(index) => {
                    // if we have exceeded the max search depth, stop traversing tree
                    if self.nodes.try_push(node_ptr).is_err() {
                        break;
                    }
                    current_node = node.children.get_mut(index).map(|e| (&mut **e) as *mut _);
                },
                Err(e) => return Some(e),
            }
        }
        None
    }

    fn pop_node(&mut self) -> Option<&'a mut TreeNode<K, V>> {
        // Safety: this requires mutable reference to the node visitor,
        // so only 1 mutable tree node can exist at a time
        unsafe { Some(self.nodes.pop()?.as_mut().unwrap()) }
    }
}

pub struct BTreeMap<K: Ord, V> {
    root: Option<Box<TreeNode<K, V>>>,
    len: usize,
    allocer: HeapRef,
}

impl<K: Ord, V> BTreeMap<K, V> {
    pub fn new(allocator: HeapRef) -> Self {
        BTreeMap {
            root: None,
            len: 0,
            allocer: allocator,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut current_node = self.root.as_ref()?;
        loop {
            match current_node.locate_element(key) {
                LocateResult::Value(i) => return Some(&current_node.values[i].1),
                LocateResult::SubTree(i) => current_node = &current_node.children[i],
                LocateResult::None => return None,
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> KResult<Option<V>> {
        let mut node_visitor = NodeVisitor::new(self);
        let locate_result = node_visitor.visit_nodes(|node| match node.locate_element(&key) {
            LocateResult::Value(i) => Err(i),
            LocateResult::SubTree(i) => Ok(i),
            LocateResult::None => Ok(0),
        });

        if let Some(old_node_index) = locate_result {
            // panic safety: there must be at least 1 node in the node_visitor if locate_result is some
            let node = node_visitor.pop_node().unwrap();
            let (_, old_value) = core::mem::replace(&mut node.values[old_node_index], (key, value));
            return Ok(Some(old_value));
        }

        todo!();
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        todo!();
    }
}
