//! Native search-list cleanup releases transient INFO while obstacle and mobile
//! claims retain the original allocation's indexed coordinates.
use crate::grid::SearchResult;
use crate::skirmish::Skirmish;
use std::sync::Arc;

impl Skirmish {
    pub(in crate::skirmish) fn finish_path_search(
        &mut self,
        mut result: SearchResult,
    ) -> SearchResult {
        for (node, coordinate) in result.info_cleanup.drain(..) {
            if self.occupancy.claim_info(node) {
                Arc::make_mut(&mut self.occupancy.infos).insert(node, coordinate);
                self.occupancy
                    .release_info(node, self.navigation.layer_obstacle(node));
            }
        }
        for (node, info) in result.retained_infos.drain(..) {
            if self.occupancy.claim_info(node) {
                Arc::make_mut(&mut self.occupancy.infos).insert(node, info);
            }
        }
        result
    }
}

#[cfg(test)]
mod query_tests;
#[cfg(test)]
mod tests;
