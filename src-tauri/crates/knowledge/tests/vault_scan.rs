use knowledge::{inspect_vault, scan_vault};
use std::fs;

fn directory_fingerprint(root: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    use std::time::UNIX_EPOCH;

    let mut rows = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let metadata = entry.metadata().unwrap();
            let modified = metadata
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            format!("{relative}\t{}\t{modified}", metadata.len())
        })
        .collect::<Vec<_>>();
    rows.sort();
    format!("{:x}", Sha256::digest(rows.join("\n").as_bytes()))
}

fn write(root: &std::path::Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn inspects_existing_vault_without_writing_to_it() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    let before = fs::read_dir(root.path()).unwrap().count();
    let result = inspect_vault(root.path()).unwrap();
    assert!(result.valid);
    assert_eq!(result.markdown_count, 0);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), before);
}

#[test]
fn indexes_only_approved_cards_and_reports_invalid_frontmatter() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    write(root.path(), "01-产品事实/PF-001.md", "---\nid: PF-001\ntitle: R7 产品事实\ntype: product_fact\nstatus: 已审核\nversion: 2\n---\n# 正文");
    write(root.path(), "03-直播案例/LC-001.md", "---\nid: LC-001\ntitle: 待审核案例\ntype: live_case\nstatus: 待确认\nversion: 1\n---\n正文");
    write(
        root.path(),
        "03-直播案例/broken.md",
        "---\nid: [\n---\n正文",
    );
    write(root.path(), "03-直播案例/private.md", "---\nid: LC-PRIVATE\ntitle: 客户记录\ntype: live_case\nstatus: approved\nversion: 1\nsensitivity: personal\n---\n手机号 13800138000");
    write(
        root.path(),
        "90-模板/template.md",
        "---\nid: TEMPLATE\nstatus: 已审核\n---\n模板",
    );

    let scan = scan_vault(root.path()).unwrap();
    assert_eq!(scan.documents.len(), 4);
    assert_eq!(
        scan.documents.iter().filter(|item| item.eligible).count(),
        1
    );
    assert_eq!(scan.issues.len(), 2);
    let private = scan
        .documents
        .iter()
        .find(|item| item.card_id == "LC-PRIVATE")
        .unwrap();
    assert!(private.body.is_empty());
    assert_eq!(
        private.issue.as_deref(),
        Some("检测到个人或受限信息，正文未进入索引")
    );
    assert!(!root.path().join("99-待处理冲突").exists());
}

#[test]
fn rejects_paths_that_are_not_directories() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-vault.md");
    fs::write(&file, "x").unwrap();
    assert!(inspect_vault(&file)
        .unwrap_err()
        .to_string()
        .contains("文件夹"));
}

#[test]
fn duplicate_card_ids_are_reported_and_never_eligible() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    for name in ["first.md", "second.md"] {
        write(root.path(), name, "---\nid: DUP-001\ntitle: 重复卡\ntype: product_fact\nstatus: approved\nversion: 1\n---\n正文");
    }
    let scan = scan_vault(root.path()).unwrap();
    assert_eq!(scan.issues.len(), 2);
    assert!(scan.documents.iter().all(|item| !item.eligible));
    assert!(scan
        .documents
        .iter()
        .all(|item| item.issue.as_deref() == Some("知识卡 ID 重复: DUP-001")));
}

#[test]
#[ignore = "requires OBSIDIAN_TEST_VAULT"]
fn scans_external_vault_without_modifying_files() {
    let path = std::env::var_os("OBSIDIAN_TEST_VAULT").expect("OBSIDIAN_TEST_VAULT");
    let root = std::path::PathBuf::from(path);
    let before = directory_fingerprint(&root);
    let scan = scan_vault(&root).unwrap();
    let after = directory_fingerprint(&root);
    println!(
        "documents={} eligible={} issues={} fingerprint_before={} fingerprint_after={}",
        scan.documents.len(),
        scan.documents.iter().filter(|item| item.eligible).count(),
        scan.issues.len(),
        before,
        after
    );
    assert!(!scan.documents.is_empty());
    assert!(scan
        .documents
        .iter()
        .any(|item| item.status == "pending_review"));
    assert_eq!(before, after);
}
