use std::collections::BTreeMap;

pub type Criteria = BTreeMap<String, u64>;

pub type CriteriaGroup = BTreeMap<String, Criteria>;

pub type CriteriaGroups = BTreeMap<String, CriteriaGroup>;

pub fn combine_maps(map1: Criteria, map2: Criteria) -> Criteria {
    let mut combined_map = map1;
    for (key, value) in map2 {
        *combined_map.entry(key).or_insert(0) += value;
    }
    combined_map
}

pub fn combine_criteria_groups(group1: CriteriaGroup, group2: CriteriaGroup) -> CriteriaGroup {
    let mut combined_group = group1;

    for (key, criteria) in group2 {
        let maybe_criteria = combined_group.get(&key);
        match maybe_criteria {
            Some(existing_criteria) => {
                combined_group.insert(key, combine_maps(existing_criteria.clone(), criteria));
            }
            None => {
                combined_group.insert(key, criteria);
            }
        }
    }
    combined_group
}

pub fn combine_groups_of_criteria_groups(groups1: CriteriaGroups, groups2: CriteriaGroups) -> CriteriaGroups {
    let mut combined_groups = groups1;

    for (key, criteria_group) in groups2 {
        let maybe_criteria_group = combined_groups.get(&key);
        match maybe_criteria_group {
            Some(existing_criteria_group) => {
                combined_groups.insert(key, combine_criteria_groups(existing_criteria_group.clone(), criteria_group));
            }
            None => {
                combined_groups.insert(key, criteria_group);
            }
        }
    }
    combined_groups
}


#[cfg(test)]
mod test {
    use super::*;

    const CRITERIA_GROUP_JSON: &str = r#"{"gender":{"female":20,"male":20,"other":10}}"#;
    const CRITERIA_GROUPS_JSON: &str = r#"{"patients":{"gender":{"female":20,"male":20,"other":10}}}"#;

    #[test]
    fn test_combining_criteria_groups_serialization() {
        let map1: Criteria = [("male".into(), 20), ("female".into(), 10)]
            .iter()
            .cloned()
            .collect();

        let map2: Criteria = [("female".into(), 10), ("other".into(), 10)]
            .iter()
            .cloned()
            .collect();

        let combined_map = combine_maps(map1.clone(), map2.clone());

        let criteria_group: CriteriaGroup =
            [("gender".into(), combined_map)].iter().cloned().collect();

        let criteria_group_json =
            serde_json::to_string(&criteria_group).expect("Failed to serialize JSON");

        pretty_assertions::assert_eq!(CRITERIA_GROUP_JSON, criteria_group_json);

        let criteria_group1: CriteriaGroup =
        [("gender".into(), map1)].iter().cloned().collect();

        let criteria_group2: CriteriaGroup =
        [("gender".into(), map2)].iter().cloned().collect();

        let criteria_group_combined = combine_criteria_groups(criteria_group1.clone(), criteria_group2.clone());

        let criteria_group_combined_json =  serde_json::to_string(&criteria_group_combined).expect("Failed to serialize JSON");
        
        pretty_assertions::assert_eq!(CRITERIA_GROUP_JSON, criteria_group_combined_json);

        let criteria_groups1 : CriteriaGroups = 
        [("patients".into(), criteria_group1)].iter().cloned().collect();

        let criteria_groups2 : CriteriaGroups = 
        [("patients".into(), criteria_group2)].iter().cloned().collect();

        let criteria_groups_combined : CriteriaGroups = combine_groups_of_criteria_groups(criteria_groups1, criteria_groups2);

        let criteria_groups_combined_json =  serde_json::to_string(&criteria_groups_combined).expect("Failed to serialize JSON");

        pretty_assertions::assert_eq!(CRITERIA_GROUPS_JSON, criteria_groups_combined_json);


    }
}
