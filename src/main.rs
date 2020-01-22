
use std::path::Path;
use std::env;
use git2::{Repository, Error, Config, Oid};

fn create_initial_commit(repo: &Repository) -> Result<(), Error> {
    // First use the config to initialize a commit signature for the user.
    let sig = repo.signature()?;

    // Now let's create an empty tree for this commit
    let tree_id = {
        let mut index = repo.index()?;

        // Outside of this example, you could call index.add_path()
        // here to put actual files into the index. For our purposes, we'll
        // leave it empty for now.

        index.write_tree()?
    };

    let tree = repo.find_tree(tree_id)?;

    // Ready to create the initial commit.
    //
    // Normally creating a commit would involve looking up the current HEAD
    // commit and making that be the parent of the initial commit, but here this
    // is the first commit so there will be no parent.
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;

    Ok(())
}

fn create_checkout_branch(repo: &Repository, br_name: &str, oid_str: Option<&str>) -> Result<(), Error> {
    let oid: Oid;
    if oid_str == None {
        let head = repo.head()?;
        oid = head.target().unwrap();
    } else {
        oid = Oid::from_str(oid_str.unwrap())?;
    }
    let commit = repo.find_commit(oid)?;
    repo.branch(br_name, &commit, false)?;
    let mut refs_tree = String::from("refs/heads/");
    refs_tree.push_str(br_name);
    let obj = repo.revparse_single(&refs_tree)?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&refs_tree)?;

    Ok(())
}

fn gf_init<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let repo = Repository::init(path)?;
    let mut config_l = repo.config()?;

    // create an initial commit for master branch
    create_initial_commit(&repo)?;
    config_l.set_str("gitflow.branch.master", "master")?;

    // git checkout -b develop
    create_checkout_branch(&repo, "develop", None)?;
    config_l.set_str("gitflow.branch.develop", "develop")?;

    config_l.set_str("gitflow.prefix.feature", "feature")?;
    config_l.set_str("gitflow.prefix.release", "release")?;
    config_l.set_str("gitflow.prefix.hotfix", "hotfix")?;
    config_l.set_str("gitflow.prefix.support", "support")?;
    config_l.set_str("gitflow.prefix.versiontag", "")?;

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = Path::new(&args[1]);

    let config_g = match Config::open_default() {
        Ok(config_g) => config_g,
        Err(e) => panic!("{}", e)
    };

    match gf_init(&path) {
        Ok(()) => println!("run gf_init success"),
        Err(e) => panic!("{}", e),
    }
}
