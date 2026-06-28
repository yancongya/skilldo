use std::fs;

use std::process::Command;

use crate::core::git_fetcher::{clone_or_pull, clone_or_pull_sparse, commit_all_and_push};

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn run_git(args: &[&str], cwd: Option<&std::path::Path>) {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit_file(repo: &git2::Repository, path: &str, content: &[u8], msg: &str) -> git2::Oid {
    let workdir = repo.workdir().expect("workdir");
    let file_path = workdir.join(path);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&file_path, content).unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(path)).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    let parents = match repo.head() {
        Ok(head) => vec![repo.find_commit(head.target().unwrap()).unwrap()],
        Err(_) => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, parent_refs.as_slice())
        .unwrap()
}

#[test]
fn clone_then_pull_updates_head() {
    let origin_dir = tempfile::tempdir().unwrap();
    let origin = git2::Repository::init(origin_dir.path()).unwrap();
    let _c1 = commit_file(&origin, "a.txt", b"v1", "c1");
    let c2 = commit_file(&origin, "a.txt", b"v2", "c2");

    let dest_dir = tempfile::tempdir().unwrap();
    let dest = dest_dir.path().join("clone");

    let h1 = clone_or_pull(
        origin_dir.path().to_string_lossy().as_ref(),
        &dest,
        None,
        None,
    )
    .unwrap();
    assert_eq!(h1, c2.to_string(), "首次 clone 应指向最新提交");

    let c3 = commit_file(&origin, "b.txt", b"v3", "c3");
    let h2 = clone_or_pull(
        origin_dir.path().to_string_lossy().as_ref(),
        &dest,
        None,
        None,
    )
    .unwrap();
    assert_eq!(h2, c3.to_string(), "再次调用应更新到最新提交");
}

#[test]
fn sparse_clone_only_materializes_requested_subpath() {
    let origin_dir = tempfile::tempdir().unwrap();
    let origin = git2::Repository::init(origin_dir.path()).unwrap();
    let _ = commit_file(&origin, "skills/a/SKILL.md", b"---\nname: A\n---\n", "c1");
    let _ = commit_file(&origin, "skills/b/SKILL.md", b"---\nname: B\n---\n", "c2");

    let dest_dir = tempfile::tempdir().unwrap();
    let dest = dest_dir.path().join("clone");

    let head = match clone_or_pull_sparse(
        origin_dir.path().to_string_lossy().as_ref(),
        &dest,
        None,
        "skills/a",
        None,
    ) {
        Ok(head) => head,
        Err(err) if format!("{:#}", err).contains("system git is required") => return,
        Err(err) => panic!("sparse clone failed: {:#}", err),
    };

    assert!(!head.is_empty());
    assert!(dest.join("skills/a/SKILL.md").exists());
    assert!(
        !dest.join("skills/b/SKILL.md").exists(),
        "未请求的子目录不应被检出到工作区"
    );
}

#[test]
fn commit_all_and_push_pushes_changes_to_origin() {
    if !git_available() {
        return;
    }

    let root = tempfile::tempdir().unwrap();
    let origin = root.path().join("origin.git");
    let seed = root.path().join("seed");
    let work = root.path().join("work");
    let verify = root.path().join("verify");

    run_git(&["init", "--bare", origin.to_string_lossy().as_ref()], None);
    run_git(
        &[
            "clone",
            origin.to_string_lossy().as_ref(),
            seed.to_string_lossy().as_ref(),
        ],
        None,
    );
    run_git(&["config", "user.name", "Skills Hub"], Some(&seed));
    run_git(&["config", "user.email", "skills@example.com"], Some(&seed));
    fs::write(seed.join("SKILL.md"), b"---\nname: Test\n---\nold\n").unwrap();
    run_git(&["add", "SKILL.md"], Some(&seed));
    run_git(&["commit", "-m", "seed"], Some(&seed));
    run_git(&["push", "origin", "HEAD:main"], Some(&seed));

    run_git(
        &[
            "clone",
            origin.to_string_lossy().as_ref(),
            work.to_string_lossy().as_ref(),
        ],
        None,
    );
    run_git(&["checkout", "main"], Some(&work));
    run_git(&["config", "user.name", "Skills Hub"], Some(&work));
    run_git(&["config", "user.email", "skills@example.com"], Some(&work));
    fs::write(work.join("SKILL.md"), b"---\nname: Test\n---\nnew\n").unwrap();

    let result = commit_all_and_push(&work, Some("main"), "update skill").unwrap();
    assert!(result.pushed);
    assert!(result.commit.is_some());

    run_git(
        &[
            "clone",
            origin.to_string_lossy().as_ref(),
            verify.to_string_lossy().as_ref(),
        ],
        None,
    );
    run_git(&["checkout", "main"], Some(&verify));
    assert_eq!(
        fs::read_to_string(verify.join("SKILL.md")).unwrap(),
        "---\nname: Test\n---\nnew\n"
    );

    let no_changes = commit_all_and_push(&work, Some("main"), "update skill").unwrap();
    assert!(!no_changes.pushed);
    assert!(no_changes.commit.is_none());
}
