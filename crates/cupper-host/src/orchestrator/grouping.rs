//! A turn's activity groups and final answer (design 5.3.5; desktop #8, #9).
//!
//! - Consecutive tool calls of a turn share a group, and a thought among
//!   them joins it. Any other item (a message, a request, an error, a notice,
//!   a plan, a steered message, late output) ends the group; late output
//!   never joins one.
//! - The final answer is the agent messages after the last process item (a
//!   tool call or a grouped thought), or all of them when the turn has none.
//!   Nothing else takes part.

use cupper_protocol::ids::{GroupId, ItemId};

/// How an item takes part in grouping and in the final answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Message,
    Thought,
    ToolCall,
    /// Any other item; it ends the group.
    Other,
}

/// One turn's items so far.
#[derive(Debug, Default)]
pub struct TurnItems {
    group: Option<GroupId>,
    /// The messages since the last process item.
    answer: Vec<ItemId>,
}

impl TurnItems {
    /// The group of `item`, which starts now; `mint` names a new group.
    pub fn start(
        &mut self,
        item: &ItemId,
        role: Role,
        mint: impl FnOnce() -> GroupId,
    ) -> Option<GroupId> {
        match role {
            Role::ToolCall => {
                self.answer.clear();
                return Some(self.group.get_or_insert_with(mint).clone());
            }
            Role::Thought if self.group.is_some() => {
                self.answer.clear();
                return self.group.clone();
            }
            Role::Thought => return None,
            Role::Message => self.answer.push(item.clone()),
            Role::Other => {}
        }
        self.group = None;
        None
    }

    /// Something other than an item goes into the turn: a request.
    pub fn end_group(&mut self) {
        self.group = None;
    }

    /// `turn.completed.final_item_ids`.
    pub fn final_items(&self) -> Vec<ItemId> {
        self.answer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use Role::{Message as M, Other as O, Thought as T, ToolCall as C};

    /// Starts an item per role, `None` standing for a request. Returns each
    /// item's group, numbered from 1 in the order minted (0 for none), and
    /// the positions of the final answer.
    fn run(roles: &[Option<Role>]) -> (Vec<usize>, Vec<usize>) {
        let mut items = TurnItems::default();
        let ids: Vec<ItemId> = (0..roles.len() as u128).map(ItemId::from_uuid).collect();
        let (mut minted, mut groups) = (Vec::new(), Vec::new());
        for (id, role) in ids.iter().zip(roles) {
            let group = match role {
                Some(role) => items.start(id, *role, || {
                    minted.push(GroupId::from_uuid(minted.len() as u128));
                    minted[minted.len() - 1].clone()
                }),
                None => {
                    items.end_group();
                    None
                }
            };
            groups.push(group.map_or(0, |g| 1 + minted.iter().position(|m| *m == g).unwrap()));
        }
        let at = |item: &ItemId| ids.iter().position(|id| id == item).unwrap();
        (groups, items.final_items().iter().map(at).collect())
    }

    #[test]
    fn unit_grouping_tool_calls_share_a_group_that_thoughts_keep() {
        let (groups, _) = run(&[Some(T), Some(C), Some(T), Some(C), Some(M), Some(C)]);
        assert_eq!(groups, [0, 1, 1, 1, 0, 2]);
    }

    #[test]
    fn unit_grouping_other_items_and_requests_end_the_group() {
        let (groups, _) = run(&[Some(C), Some(O), Some(C), None, Some(T), Some(C)]);
        assert_eq!(groups, [1, 0, 2, 0, 0, 3]);
    }

    #[test]
    fn unit_final_items_messages_after_the_last_process_item() {
        let (_, answer) = run(&[Some(M), Some(C), Some(M), Some(O), Some(M)]);
        assert_eq!(answer, [2, 4]);
        // A grouped thought is a process item; one outside a group is not.
        let (_, answer) = run(&[Some(C), Some(M), Some(T), Some(M), Some(C), Some(T)]);
        assert!(answer.is_empty());
        let (_, answer) = run(&[Some(C), Some(T), Some(M), Some(T), Some(M)]);
        assert_eq!(answer, [2, 4]);
    }

    #[test]
    fn unit_final_items_all_messages_without_process_items() {
        let (_, answer) = run(&[Some(T), Some(M), None, Some(O), Some(M)]);
        assert_eq!(answer, [1, 4]);
        assert!(run(&[Some(O), None]).1.is_empty());
    }
}
