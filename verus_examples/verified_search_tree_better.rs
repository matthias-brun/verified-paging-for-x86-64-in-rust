use builtin::*;
use builtin_macros::*;
mod pervasive;
use pervasive::set::*;

verus! {

struct TreeNode {
    value: usize,
    left:  Tree,
    right: Tree,
}

enum Tree {
    Leaf,
    Node(Box<TreeNode>),
}

impl Tree {
    spec fn view(self) -> Set<nat>
        decreases self
    {
        match self {
            Tree::Leaf => {
                set![]
            },
            Tree::Node(node) => {
                set![node.value as nat] + node.left.view() + node.right.view()
            },
        }
    }

    spec fn all_values_less_than(self, m: usize) -> bool
    { forall|x: usize| self@.contains(x as nat) ==> x < m }

    spec fn all_values_greater_than(self, m: usize) -> bool
    { forall|x: usize| self@.contains(x as nat) ==> x > m }

    spec fn is_search_tree(self) -> bool
        decreases self
    {
        match self {
            Tree::Leaf => {
                true
            },
            Tree::Node(node) => {
                &&& node.left.all_values_less_than(node.value)
                &&& node.right.all_values_greater_than(node.value)
                &&& node.left.is_search_tree()
                &&& node.right.is_search_tree()
            },
        }
    }

    fn contains_value(&self, m: usize) -> (r: bool)
        requires self.is_search_tree()
        ensures r == self@.contains(m as nat)
    {
        match self {
            Tree::Leaf => {
                false
            },
            Tree::Node(node) => {
                if m == node.value {
                    true
                } else if m < node.value {
                    let res = node.left.contains_value(m);
                    if !res {
                        assert(!node.right@.contains(m as nat));
                    }
                    res
                } else {
                    let res = node.right.contains_value(m);
                    if !res {
                        assert(!node.left@.contains(m as nat));
                    }
                    res
                }
            },
        }
    }

    #[verifier(external_body)]
    fn insert_value(&mut self, m: usize) {
        match self {
            Tree::Leaf => {
                let new_node = TreeNode {
                    value: m,
                    left:  Tree::Leaf,
                    right: Tree::Leaf,
                };
                *self = Tree::Node(Box::new(new_node));
            },
            Tree::Node(node) => {
                if m == node.value {
                    // value already exists
                } else if m < node.value {
                    node.left.insert_value(m);
                } else {
                    node.right.insert_value(m);
                }
            },
        }
    }
}

fn main() {
}
}
