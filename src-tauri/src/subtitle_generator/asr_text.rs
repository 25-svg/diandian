pub fn normalize_commerce_text_boundaries(input: &str) -> String {
    let mut value = input.to_string();
    for marker in ["99新", "九十九新"] {
        let mut search_from = 0;
        while let Some(offset) = value[search_from..].find(marker) {
            let at = search_from + offset;
            let previous = value[..at].chars().next_back();
            let mut suffix_start = at;
            while suffix_start > 0
                && (value.as_bytes()[suffix_start - 1].is_ascii_alphanumeric()
                    || value.as_bytes()[suffix_start - 1] == b'-')
            {
                suffix_start -= 1;
            }
            let has_ascii_model_suffix = value[suffix_start..at]
                .bytes()
                .any(|byte| byte.is_ascii_alphabetic());
            let model_words = value[..at].trim_end().split_whitespace().rev();
            let has_ascii_model_prefix = has_ascii_model_suffix
                || model_words
                .take_while(|word| {
                    word.chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                })
                .any(|word| word.chars().any(|ch| ch.is_ascii_alphabetic()));
            if previous.is_some_and(|ch| ch.is_ascii_alphanumeric())
                && !value[..at].ends_with(' ')
                && has_ascii_model_prefix
            {
                value.insert(at, ' ');
                search_from = at + marker.len() + 1;
            } else {
                search_from = at + marker.len();
            }
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::normalize_commerce_text_boundaries;

    #[test]
    fn separates_condition_grade_from_alphanumeric_model() {
        assert_eq!(
            normalize_commerce_text_boundaries("A4PRO299新的，咱们说喜欢运动相机的可以看一下"),
            "A4PRO2 99新的，咱们说喜欢运动相机的可以看一下"
        );
    }

    #[test]
    fn leaves_plain_alphanumeric_model_unchanged() {
        assert_eq!(
            normalize_commerce_text_boundaries("影石A4PRO2运动相机"),
            "影石A4PRO2运动相机"
        );
    }

    #[test]
    fn leaves_plain_number_and_condition_unchanged() {
        assert_eq!(normalize_commerce_text_boundaries("一共199新的"), "一共199新的");
    }

    #[test]
    fn separates_model_from_chinese_number_condition_phrase() {
        assert_eq!(
            normalize_commerce_text_boundaries("A4 Pro 2九十九新"),
            "A4 Pro 2 九十九新"
        );
    }

    #[test]
    fn preserves_model_and_unrelated_numbers() {
        assert_eq!(
            normalize_commerce_text_boundaries("影石A4PRO2"),
            "影石A4PRO2"
        );
        assert_eq!(normalize_commerce_text_boundaries("到手价99新币"), "到手价99新币");
        assert_eq!(normalize_commerce_text_boundaries("R50白色"), "R50白色");
    }

    #[test]
    fn separates_branded_ascii_model_from_condition_phrase() {
        assert_eq!(
            normalize_commerce_text_boundaries("影石A4PRO299新"),
            "影石A4PRO2 99新"
        );
    }
}
