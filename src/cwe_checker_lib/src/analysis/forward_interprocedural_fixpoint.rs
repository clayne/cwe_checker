//! Creating and computing forward interprocedural fixpoint problems.
//!
//! # General notes
//!
//! This module supports computation of fixpoint problems on the control flow graphs generated by the `graph` module.
//!
//! To compute a generalized fixpoint problem,
//! first construct a context object implementing the `Context`trait.
//! Use it to construct a `Computation` object.
//! The `Computation` object provides the necessary methods for the actual fixpoint computation.

use super::fixpoint::Context as GeneralFPContext;
use super::graph::*;
use super::interprocedural_fixpoint_generic::*;

use crate::intermediate_representation::*;

use std::marker::PhantomData;

use petgraph::graph::EdgeIndex;
use petgraph::graph::NodeIndex;

/// The context for an interprocedural fixpoint computation.
///
/// Basically, a `Context` object needs to contain a reference to the actual graph,
/// a method for merging node values,
/// and methods for computing the edge transitions for each different edge type.
///
/// All trait methods have access to the FixpointProblem structure, so that context informations are accessible through it.
///
/// All edge transition functions can return `None` to indicate that no information flows through the edge.
/// For example, this can be used to indicate edges that can never been taken.
pub trait Context<'a> {
    /// The type of the values that are assigned to nodes during the fixpoint computation.
    type Value: PartialEq + Eq + Clone;

    /// Get a reference to the graph that the fixpoint is computed on.
    fn get_graph(&self) -> &Graph<'a>;

    /// Merge two node values.
    fn merge(&self, value1: &Self::Value, value2: &Self::Value) -> Self::Value;

    /// Transition function for `Def` terms.
    /// The transition function for a basic block is computed
    /// by iteratively applying this function to the starting value for each `Def` term in the basic block.
    /// The iteration short-circuits and returns `None` if `update_def` returns `None` at any point.
    fn update_def(&self, value: &Self::Value, def: &Term<Def>) -> Option<Self::Value>;

    /// Transition function for (conditional and unconditional) `Jmp` terms.
    fn update_jump(
        &self,
        value: &Self::Value,
        jump: &Term<Jmp>,
        untaken_conditional: Option<&Term<Jmp>>,
        target: &Term<Blk>,
    ) -> Option<Self::Value>;

    /// Transition function for in-program calls.
    fn update_call(
        &self,
        value: &Self::Value,
        call: &Term<Jmp>,
        target: &Node,
        calling_convention: &Option<String>,
    ) -> Option<Self::Value>;

    /// Transition function for return instructions.
    /// Has access to the value at the callsite corresponding to the return edge.
    /// This way one can recover caller-specific information on return from a function.
    fn update_return(
        &self,
        value: Option<&Self::Value>,
        value_before_call: Option<&Self::Value>,
        call_term: &Term<Jmp>,
        return_term: &Term<Jmp>,
        calling_convention: &Option<String>,
    ) -> Option<Self::Value>;

    /// Transition function for calls to functions not contained in the binary.
    /// The corresponding edge goes from the callsite to the returned-to block.
    fn update_call_stub(&self, value: &Self::Value, call: &Term<Jmp>) -> Option<Self::Value>;

    /// This function is used to refine the value using the information on which branch was taken on a conditional jump.
    fn specialize_conditional(
        &self,
        value: &Self::Value,
        condition: &Expression,
        block_before_condition: &Term<Blk>,
        is_true: bool,
    ) -> Option<Self::Value>;
}

/// This struct is a wrapper to create a general fixpoint context out of an interprocedural fixpoint context.
pub struct GeneralizedContext<'a, T: Context<'a>> {
    context: T,
    _phantom_graph_reference: PhantomData<Graph<'a>>,
}

impl<'a, T: Context<'a>> GeneralizedContext<'a, T> {
    /// Create a new generalized context out of an interprocedural context object.
    pub fn new(context: T) -> Self {
        GeneralizedContext {
            context,
            _phantom_graph_reference: PhantomData,
        }
    }

    /// Get the inner context object.
    pub fn get_context(&self) -> &T {
        &self.context
    }
}

impl<'a, T: Context<'a>> GeneralFPContext for GeneralizedContext<'a, T> {
    type EdgeLabel = Edge<'a>;
    type NodeLabel = Node<'a>;
    type NodeValue = NodeValue<T::Value>;

    /// Get a reference to the underlying graph.
    fn get_graph(&self) -> &Graph<'a> {
        self.context.get_graph()
    }

    /// Merge two values using the merge function from the interprocedural
    /// context object.
    fn merge(&self, val1: &Self::NodeValue, val2: &Self::NodeValue) -> Self::NodeValue {
        use NodeValue::*;
        match (val1, val2) {
            (Value(value1), Value(value2)) => Value(self.context.merge(value1, value2)),
            (
                CallFlowCombinator {
                    call_stub: call1,
                    interprocedural_flow: return1,
                },
                CallFlowCombinator {
                    call_stub: call2,
                    interprocedural_flow: return2,
                },
            ) => CallFlowCombinator {
                call_stub: merge_option(call1, call2, |v1, v2| self.context.merge(v1, v2)),
                interprocedural_flow: merge_option(return1, return2, |v1, v2| {
                    self.context.merge(v1, v2)
                }),
            },
            _ => panic!("Malformed CFG in fixpoint computation"),
        }
    }

    /// Forward edge transition function.
    /// Applies the transition functions from the interprocedural context object
    /// corresponding to the type of the provided edge.
    fn update_edge(
        &self,
        node_value: &Self::NodeValue,
        edge: EdgeIndex,
    ) -> Option<Self::NodeValue> {
        let graph = self.context.get_graph();
        let (start_node, end_node) = graph.edge_endpoints(edge).unwrap();

        match graph.edge_weight(edge).unwrap() {
            Edge::Block => {
                let block_term = graph.node_weight(start_node).unwrap().get_block();
                let value = node_value.unwrap_value();
                let defs = &block_term.term.defs;
                let end_val = defs.iter().try_fold(value.clone(), |accum, def| {
                    self.context.update_def(&accum, def)
                });
                end_val.map(NodeValue::Value)
            }
            Edge::CallCombine(_) => Some(Self::NodeValue::Value(node_value.unwrap_value().clone())),
            Edge::Call(call) => self
                .context
                .update_call(
                    node_value.unwrap_value(),
                    call,
                    &graph[end_node],
                    &graph[end_node].get_sub().term.calling_convention,
                )
                .map(NodeValue::Value),
            Edge::CrCallStub => Some(NodeValue::CallFlowCombinator {
                call_stub: Some(node_value.unwrap_value().clone()),
                interprocedural_flow: None,
            }),
            Edge::CrReturnStub => Some(NodeValue::CallFlowCombinator {
                call_stub: None,
                interprocedural_flow: Some(node_value.unwrap_value().clone()),
            }),
            Edge::ReturnCombine(call_term) => match node_value {
                NodeValue::Value(_) => panic!("Unexpected interprocedural fixpoint graph state"),
                NodeValue::CallFlowCombinator {
                    call_stub,
                    interprocedural_flow,
                } => {
                    let (return_from_block, return_from_sub) = match graph.node_weight(start_node) {
                        Some(Node::CallReturn {
                            call: _,
                            return_: (return_from_block, return_from_sub),
                        }) => (return_from_block, return_from_sub),
                        _ => panic!("Malformed Control flow graph"),
                    };
                    let return_from_jmp = &return_from_block.term.jmps[0];
                    self.context
                        .update_return(
                            interprocedural_flow.as_ref(),
                            call_stub.as_ref(),
                            call_term,
                            return_from_jmp,
                            &return_from_sub.term.calling_convention,
                        )
                        .map(NodeValue::Value)
                }
            },
            Edge::ExternCallStub(call) => self
                .context
                .update_call_stub(node_value.unwrap_value(), call)
                .map(NodeValue::Value),
            Edge::Jump(jump, untaken_conditional) => {
                let value_after_condition = if let Jmp::CBranch {
                    target: _,
                    condition,
                } = &jump.term
                {
                    let block = graph[start_node].get_block();
                    self.context.specialize_conditional(
                        node_value.unwrap_value(),
                        condition,
                        block,
                        true,
                    )
                } else if let Some(untaken_conditional_jump) = untaken_conditional {
                    if let Jmp::CBranch {
                        target: _,
                        condition,
                    } = &untaken_conditional_jump.term
                    {
                        let block = graph[start_node].get_block();
                        self.context.specialize_conditional(
                            node_value.unwrap_value(),
                            condition,
                            block,
                            false,
                        )
                    } else {
                        panic!("Malformed control flow graph");
                    }
                } else {
                    Some(node_value.unwrap_value().clone())
                };
                if let Some(value) = value_after_condition {
                    self.context
                        .update_jump(
                            &value,
                            jump,
                            *untaken_conditional,
                            graph[end_node].get_block(),
                        )
                        .map(NodeValue::Value)
                } else {
                    None
                }
            }
        }
    }
}

/// Generate a new computation from the corresponding context and an optional default value for nodes.
pub fn create_computation<'a, T: Context<'a>>(
    problem: T,
    default_value: Option<T::Value>,
) -> super::fixpoint::Computation<GeneralizedContext<'a, T>> {
    let generalized_problem = GeneralizedContext::new(problem);
    super::fixpoint::Computation::new(generalized_problem, default_value.map(NodeValue::Value))
}

/// Returns a node ordering with callee nodes behind caller nodes.
pub fn create_bottom_up_worklist(graph: &Graph) -> Vec<NodeIndex> {
    let mut graph = graph.clone();
    graph.retain_edges(|frozen, edge| !matches!(frozen[edge], Edge::Call(..)));
    petgraph::algo::kosaraju_scc(&graph)
        .into_iter()
        .flatten()
        .collect()
}

/// Returns a node ordering with caller nodes behind callee nodes.
pub fn create_top_down_worklist(graph: &Graph) -> Vec<NodeIndex> {
    let mut graph = graph.clone();
    graph.retain_edges(|frozen, edge| !matches!(frozen[edge], Edge::CrReturnStub));
    petgraph::algo::kosaraju_scc(&graph)
        .into_iter()
        .flatten()
        .collect()
}

/// Generate a new computation from the corresponding context and an optional default value for nodes.
/// Uses a bottom up worklist order when computing the fixpoint.
///
/// The worklist order prefers callee nodes before caller nodes.
pub fn create_computation_with_bottom_up_worklist_order<'a, T: Context<'a>>(
    problem: T,
    default_value: Option<T::Value>,
) -> super::fixpoint::Computation<GeneralizedContext<'a, T>> {
    let priority_sorted_nodes: Vec<NodeIndex> = create_bottom_up_worklist(problem.get_graph());
    let generalized_problem = GeneralizedContext::new(problem);
    super::fixpoint::Computation::from_node_priority_list(
        generalized_problem,
        default_value.map(NodeValue::Value),
        priority_sorted_nodes,
    )
}

/// Generate a new computation from the corresponding context and an optional default value for nodes.
/// Uses a top down worklist order when computing the fixpoint.
///
/// The worklist order prefers caller nodes before callee nodes.
pub fn create_computation_with_top_down_worklist_order<'a, T: Context<'a>>(
    problem: T,
    default_value: Option<T::Value>,
) -> super::fixpoint::Computation<GeneralizedContext<'a, T>> {
    let priority_sorted_nodes: Vec<NodeIndex> = create_top_down_worklist(problem.get_graph());
    let generalized_problem = GeneralizedContext::new(problem);
    super::fixpoint::Computation::from_node_priority_list(
        generalized_problem,
        default_value.map(NodeValue::Value),
        priority_sorted_nodes,
    )
}

// TODO: Fix tests.
/*
#[cfg(test)]
mod tests {

    use crate::{
        analysis::{
            expression_propagation::Context,
            forward_interprocedural_fixpoint::{
                create_computation_with_bottom_up_worklist_order,
                create_computation_with_top_down_worklist_order,
            },
        },
        expr,
        intermediate_representation::*,
    };
    use std::collections::{BTreeMap, HashMap};

    fn new_block(name: &str) -> Term<Blk> {
        Term {
            tid: Tid::new(name),
            term: Blk {
                defs: vec![],
                jmps: vec![],
                indirect_jmp_targets: Vec::new(),
            },
        }
    }

    /// Creates a project with one caller function of two blocks and one callee function of one block.
    fn mock_project() -> Project {
        let mut callee_block = new_block("callee block");
        callee_block.term.jmps.push(Term {
            tid: Tid::new("ret"),
            term: Jmp::Return(expr!("42:4")),
        });

        let called_function = Term {
            tid: Tid::new("called_function"),
            term: Sub {
                name: "called_function".to_string(),
                blocks: vec![callee_block],
                calling_convention: Some("_stdcall".to_string()),
            },
        };

        let mut caller_block_2 = new_block("caller_block_2");

        let mut caller_block_1 = new_block("caller_block_1");
        caller_block_1.term.jmps.push(Term {
            tid: Tid::new("call"),
            term: Jmp::Call {
                target: called_function.tid.clone(),
                return_: Some(caller_block_2.tid.clone()),
            },
        });

        caller_block_2.term.jmps.push(Term {
            tid: Tid::new("jmp"),
            term: Jmp::Branch(caller_block_1.tid.clone()),
        });

        let caller_function = Term {
            tid: Tid::new("caller_function"),
            term: Sub {
                name: "caller_function".to_string(),
                blocks: vec![caller_block_1, caller_block_2],
                calling_convention: Some("_stdcall".to_string()),
            },
        };
        let mut project = Project::mock_x64();
        project.program.term.subs = BTreeMap::from([
            (caller_function.tid.clone(), caller_function.clone()),
            (called_function.tid.clone(), called_function.clone()),
        ]);

        project
    }

    #[test]
    /// Checks if the nodes corresponding to the callee function are first in the worklist.
    fn check_bottom_up_worklist() {
        let project = mock_project();
        let graph = crate::analysis::graph::get_program_cfg(&project.program);
        let context = Context::new(&graph);
        let comp = create_computation_with_bottom_up_worklist_order(context, Some(HashMap::new()));
        // The last two nodes should belong to the callee
        for node in comp.get_worklist()[6..].iter() {
            match graph[*node] {
                crate::analysis::graph::Node::BlkStart(_, sub)
                | crate::analysis::graph::Node::BlkEnd(_, sub) => {
                    assert_eq!(sub.tid, Tid::new("called_function"))
                }
                _ => panic!(),
            }
        }
    }

    #[test]
    fn check_top_down_worklist() {
        let project = mock_project();
        let graph = crate::analysis::graph::get_program_cfg(&project.program);
        let context = Context::new(&graph);
        let comp = create_computation_with_top_down_worklist_order(context, Some(HashMap::new()));
        // The first two nodes should belong to the callee
        for node in comp.get_worklist()[..2].iter() {
            match graph[*node] {
                crate::analysis::graph::Node::BlkStart(_, sub)
                | crate::analysis::graph::Node::BlkEnd(_, sub) => {
                    assert_eq!(sub.tid, Tid::new("called_function"))
                }
                _ => panic!(),
            }
        }
    }
}
*/
