use std::collections::BTreeMap;


pub type Criteria = BTreeMap<String, u64>;
pub type CriteriaGroup = BTreeMap<String, Criteria>;

pub fn combine_maps(map1: Criteria, map2: Criteria) -> Criteria {
    let mut combined_map = map1;
    for (key, value) in map2 {
        *combined_map.entry(key).or_insert(0) += value;
    }
    combined_map
}

#[cfg(test)]
mod test {
    use super::*;

    const CRITERIA_GROUP_JSON: &str = r#"{"gender":{"female":20,"male":20,"other":10}}"#;

    #[test]
    fn test_adding_maps_serialization() {
        let map1: Criteria = [("male".into(), 20), ("female".into(), 10)].iter().cloned().collect();

        let map2: Criteria = [("female".into(), 10), ("other".into(), 10)].iter().cloned().collect();

        let combined_map = combine_maps(map1, map2);

        let criteria_group: CriteriaGroup = [("gender".into(), combined_map)].iter().cloned().collect();

        let criteria_group_json = serde_json::to_string(&criteria_group).expect("Failed to serialize JSON");

        pretty_assertions::assert_eq!(CRITERIA_GROUP_JSON, criteria_group_json);
    }
}
