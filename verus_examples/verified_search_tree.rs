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
        decreases self
    {
        match self {
            Tree::Leaf => {
                true
            },
            Tree::Node(node) => {
                &&& node.value < m
                &&& node.left.all_values_less_than(m)
                &&& node.right.all_values_less_than(m)
            },
        }
    }

    spec fn all_values_greater_than(self, m: usize) -> bool
        decreases self
    {
        match self {
            Tree::Leaf => {
                true
            },
            Tree::Node(node) => {
                &&& node.value > m
                &&& node.left.all_values_greater_than(m)
                &&& node.right.all_values_greater_than(m)
            },
        }
    }

    proof fn lemma_transitivity(self, m: usize)
        ensures 
            forall|x: usize| self.all_values_greater_than(m) && m > x ==> self.all_values_greater_than(x),
            forall|x: usize| self.all_values_less_than(m)    && m < x ==> self.all_values_less_than(x),
        decreases self
    {
        match self {
            Tree::Leaf => {
            },
            Tree::Node(node) => {
                node.left.lemma_transitivity(m);
                node.right.lemma_transitivity(m);
                assert forall|x|
                    self.all_values_greater_than(m) && m > x
                    implies
                    self.all_values_greater_than(x) by
                {
                    assert(node.left.all_values_greater_than(x));
                    assert(node.right.all_values_greater_than(x));
                };
                assert forall|x|
                    self.all_values_less_than(m) && m < x
                    implies
                    self.all_values_less_than(x) by
                {
                    assert(node.left.all_values_less_than(x));
                    assert(node.right.all_values_less_than(x));
                };
            },
        }
    }

    proof fn lemma_search_tree_less(self, m: usize)
        requires
            self.is_search_tree(),
            self.all_values_less_than(m),
        ensures 
             !self@.contains(m as nat),
        decreases self
    {
        match self {
            Tree::Leaf => {
            },
            Tree::Node(node) => {
                node.left.lemma_search_tree_less(m);
                node.right.lemma_search_tree_less(m);
            },
        }
    }

    proof fn lemma_search_tree_greater(self, m: usize)
        requires
            self.is_search_tree(),
            self.all_values_greater_than(m),
        ensures 
             !self@.contains(m as nat),
        decreases self
    {
        match self {
            Tree::Leaf => {
            },
            Tree::Node(node) => {
                node.left.lemma_search_tree_greater(m);
                node.right.lemma_search_tree_greater(m);
            },
        }
    }

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
                    proof {
                        node.right.lemma_transitivity(node.value);
                        node.right.lemma_search_tree_greater(m);
                    }
                    node.left.contains_value(m)
                } else {
                    proof {
                        node.left.lemma_transitivity(node.value);
                        node.left.lemma_search_tree_less(m);
                    }
                    node.right.contains_value(m)
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
