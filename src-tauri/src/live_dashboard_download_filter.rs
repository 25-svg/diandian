use std::path::Path;

pub fn is_official_live_dashboard_export(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if name.starts_with("~$") {
        return false;
    }
    let Some(stem) = name.strip_suffix(".xlsx") else {
        return false;
    };
    let Some((_, suffix)) = stem.rsplit_once("_整场数据下载") else {
        return false;
    };
    suffix.is_empty()
        || suffix
            .strip_prefix(" (")
            .and_then(|value| value.strip_suffix(')'))
            .is_some_and(|number| !number.is_empty() && number.chars().all(|value| value.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::is_official_live_dashboard_export;
    use std::path::Path;

    #[test]
    fn selects_only_official_full_session_exports() {
        assert!(is_official_live_dashboard_export(Path::new(
            "直播间详情页_金典拍拍相机专卖店_2026-07-28_08-15-49_整场数据下载.xlsx"
        )));
        assert!(is_official_live_dashboard_export(Path::new(
            "直播间详情页_金典拍拍相机专卖店_2026-07-28_08-15-49_整场数据下载 (1).xlsx"
        )));
        assert!(!is_official_live_dashboard_export(Path::new(
            "直播复盘_渠道分析_0730_2356.xlsx"
        )));
        assert!(!is_official_live_dashboard_export(Path::new(
            "~$直播间详情页_店铺_整场数据下载.xlsx"
        )));
        assert!(!is_official_live_dashboard_export(Path::new(
            "直播间详情页_店铺_整场数据下载.xls"
        )));
    }
}
