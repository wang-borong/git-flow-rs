extern crate clap;

use std::path::Path;
use std::process::Command;
use std::fs::File;
use std::io::{self, Read, Write};
use std::env;
use git2::*;
use clap::{Arg, App, SubCommand};

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

fn checkout_branch(repo: &Repository, br_name: &str) -> Result<(), Error> {
    let refs_tree = &("refs/heads/".to_owned() + br_name);
    let obj = repo.revparse_single(&refs_tree)?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&refs_tree)?;

    Ok(())
}

fn create_checkout_branch(repo: &Repository, br_name: &str, base_br: Option<&str>, oid_str: Option<&str>) -> Result<(), Error> {
    let oid: Oid;
    if oid_str == None {
        if base_br != None {
            let base_br_ref = &("refs/heads/".to_owned() + base_br.unwrap());
            // set head to the base branch
            repo.set_head(base_br_ref)?;
        }
        let head = repo.head()?;
        oid = head.target().unwrap();
    } else {
        oid = Oid::from_str(oid_str.unwrap())?;
    }
    let commit = repo.find_commit(oid)?;
    repo.branch(br_name, &commit, false)?;

    checkout_branch(&repo, br_name)?;

    Ok(())
}

fn find_last_commit(repo: &Repository) -> Result<Commit, Error> {
    let obj = repo.head()?.resolve()?.peel(ObjectType::Commit)?;
    obj.into_commit().map_err(|_| Error::from_str("Couldn't find commit"))
}

fn fastforward_merge_branch(repo: &Repository, our_br: &str, their_br: &str) -> Result<(), Error> {
    let their_oid = repo.refname_to_id(&("refs/heads/".to_owned() + their_br))?;
    let our_refname = &("refs/heads/".to_owned() + our_br);
    let mut our_ref = repo.find_reference(our_refname)?;

    our_ref.set_target(their_oid, "fastforward merging")?;

    // check it out after set_target
    //checkout_branch(&repo, our_br)?;

    Ok(())
}

fn normal_merge_branch(repo: &Repository, our_br: &str, their_br: &str) -> Result<(), Error> {
    let their_oid = repo.refname_to_id(&("refs/heads/".to_owned() + their_br))?;
    let their_commit = repo.find_commit(their_oid)?;
    let their_annotated_commit = repo.find_annotated_commit(their_oid)?;
    //println!("their_commit {:?}", their_commit);

    checkout_branch(&repo, our_br)?;
    repo.merge(&[&their_annotated_commit], None, None)?;
    let parent = find_last_commit(&repo)?;
    //println!("parent {:?}", parent);

    //git commit
    let sig = repo.signature()?;
    let tree_id = {
        let mut index = repo.index()?;

        index.write_tree()?
    };

    let tree = repo.find_tree(tree_id)?;

    let editor = env::var("EDITOR").unwrap_or("nvim".into());
    Command::new(editor)
        .arg(".git/COMMIT_EDITMSG")
        .spawn()
        .expect("Spawn nvim failed")
        .wait()
        .expect("Waiting nvim failed");

    let mut msg = String::new();
    let mut f = File::open(".git/COMMIT_EDITMSG").expect("Unable to open file");
    f.read_to_string(&mut msg).expect("Unable to read string");

    repo.commit(Some("HEAD"), &sig, &sig, &msg, &tree, &[&parent, &their_commit])?;

    // reslove conflicts and merging
    repo.cleanup_state()?;

    Ok(())
}

fn merge_branch(repo: &Repository, our_br: &str, their_br: &str, ff: bool) -> Result<(), Error> {

    if ff {
        fastforward_merge_branch(&repo, our_br, their_br)?;
    } else {
        normal_merge_branch(&repo, our_br, their_br)?;
    }

    // checkout to base branch
    checkout_branch(&repo, our_br)?;

    Ok(())
}

fn delete_branch(repo: &Repository, br_name: &str) -> Result<(), Error> {
    let mut branch = repo.find_branch(&br_name, BranchType::Local)?;
    branch.delete()?;

    Ok(())
}

fn gf_init<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let repo = Repository::init(path)?;
    let mut config_l = repo.config()?;

    // create an initial commit for master branch
    create_initial_commit(&repo)?;
    config_l.set_str("gitflow.branch.master", "master")?;

    // git checkout -b develop master
    create_checkout_branch(&repo, "develop", Some("master"), None)?;
    config_l.set_str("gitflow.branch.develop", "develop")?;

    config_l.set_str("gitflow.prefix.feature", "feature/")?;
    config_l.set_str("gitflow.prefix.release", "release/")?;
    config_l.set_str("gitflow.prefix.hotfix", "hotfix/")?;
    config_l.set_str("gitflow.prefix.support", "support/")?;
    config_l.set_str("gitflow.prefix.versiontag", "")?;

    Ok(())
}

fn get_input(prompt: &str) -> String {
    print!("{}: ", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_goes_into_input_above) => {},
        Err(_no_updates_is_fine) => {},
    }
    input.trim().to_string()
}

fn create_tag(repo: &Repository, br: &str) -> Result<Oid, Error> {
    let br_oid = repo.refname_to_id(&("refs/heads/".to_owned() + br))?;
    let br_obj = repo.find_object(br_oid, None)?;

    // get a user input tag
    let tagname = get_input("Input a tag name");

    let editor = env::var("EDITOR").unwrap_or("nvim".into());
    Command::new(editor)
        .arg(".git/TAG_EDITMSG")
        .spawn()
        .expect("Spawn nvim failed")
        .wait()
        .expect("Waiting nvim failed");

    let mut msg = String::new();
    let mut f = File::open(".git/TAG_EDITMSG").expect("Unable to open file");
    f.read_to_string(&mut msg).expect("Unable to read string");

    let sig = repo.signature()?;
    let tag_oid = repo.tag(&tagname,
        &br_obj,
        &sig,
        &msg,
        true)?;

    Ok(tag_oid)
}

fn gf_subcmd(cmd: &str, subcmd: &str, base_br: &str, br: &str) -> Result<(), Error> {
    let repo = Repository::open(".")?;
    let config_l = repo.config()?;

    let prefix_conf = &("gitflow.prefix.".to_owned() + cmd);
    let prefix = config_l.get_string(prefix_conf)?;
    let br_name = &(prefix + br);

    let mut ff = true;
    match subcmd {
        "start" => create_checkout_branch(&repo, &br_name, Some(&base_br), None)?,
        "finish" => {
            if cmd == "release" || cmd == "hotfix" {
                ff = false;
                merge_branch(&repo, "master", br_name, ff)?;
                let _tag_oid = create_tag(&repo, "master")?;
                //merge_tag(&repo, base_br, tag_oid)?;
                merge_branch(&repo, base_br, "master", ff)?;
            } else {
                merge_branch(&repo, base_br, br_name, ff)?;
            }
            delete_branch(&repo, br_name)?;
        }
        _ => println!("Not implement {} for {}", subcmd, cmd),
    }

    Ok(())
}

fn gf_run() {
    let matches = App::new("git-flow")
        .version("0.1")
        .author("Jason Wang <wang_borong@163.com>")
        .about("git flow")
        // Init subcommand
        .subcommand(SubCommand::with_name("init")
            .about("git flow init")
            .arg(Arg::with_name("init_path")
                .help("path to be initialized")))
        // Feature subcommand
        .subcommand(SubCommand::with_name("feature")
            .about("git flow feature")
            .subcommand(SubCommand::with_name("start")
                .about("feature start command")
                .arg(Arg::with_name("feature_name")
                    .help("work on a feature branch")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("finish")
                .about("feature finish command")
                .arg(Arg::with_name("feature_name")
                    .help("work off a feature branch")
                    .required(true)
                    .index(1)))
        )
        // Release subcommand
        .subcommand(SubCommand::with_name("release")
            .about("git flow release")
            .subcommand(SubCommand::with_name("start")
                .about("release start command")
                .arg(Arg::with_name("release_name")
                    .help("work on a release branch")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("finish")
                .about("release finish command")
                .arg(Arg::with_name("release_name")
                    .help("work off a release branch")
                    .required(true)
                    .index(1)))
        )
        // Hotfix subcommand
        .subcommand(SubCommand::with_name("hotfix")
            .about("git flow hotfix")
            .subcommand(SubCommand::with_name("start")
                .about("hotfix start command")
                .arg(Arg::with_name("hotfix_name")
                    .help("work on a hotfix branch")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("finish")
                .about("hotfix finish command")
                .arg(Arg::with_name("hotfix_name")
                    .help("work off a hotfix branch")
                    .required(true)
                    .index(1)))
        )
        // ...
        .get_matches();

    // Init
    if let Some(matches) = matches.subcommand_matches("init") {
        let path: &str;
        if matches.is_present("init_path") {
            path = matches.value_of("init_path").unwrap();
        } else {
            path = ".";
        }
        match gf_init(&path) {
            Ok(()) => println!("Init {} Successfully", path),
            Err(e) => panic!("Init {} Failed ({})", path, e),
        }
    }

    // Feature
    if let Some(match_sub0) = matches.subcommand_matches("feature") {
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("feature_name").unwrap();
            match gf_subcmd("feature", "start", "develop", br) {
                Ok(()) => println!("Run feature {} successfully", br),
                Err(e) => panic!("Run subcmd feature failed {}", e),
            }
        }
        if let Some(match_sub1) = match_sub0.subcommand_matches("finish") {
            let br = match_sub1.value_of("feature_name").unwrap();
            match gf_subcmd("feature", "finish", "develop", br) {
                Ok(()) => println!("Run feature {} successfully", br),
                Err(e) => panic!("Run subcmd feature failed {}", e),
            }
        }
    }

    // Release
    if let Some(match_sub0) = matches.subcommand_matches("release") {
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("release_name").unwrap();
            match gf_subcmd("release", "start", "develop", br) {
                Ok(()) => println!("Run release {} successfully", br),
                Err(e) => panic!("Run subcmd release failed {}", e),
            }
        }
        if let Some(match_sub1) = match_sub0.subcommand_matches("finish") {
            let br = match_sub1.value_of("release_name").unwrap();
            match gf_subcmd("release", "finish", "develop", br) {
                Ok(()) => println!("Run release {} successfully", br),
                Err(e) => panic!("Run subcmd release failed {}", e),
            }
        }
    }

    // Hotfix
    if let Some(match_sub0) = matches.subcommand_matches("hotfix") {
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("hotfix_name").unwrap();
            match gf_subcmd("hotfix", "start", "develop", br) {
                Ok(()) => println!("Run hotfix {} successfully", br),
                Err(e) => panic!("Run subcmd hotfix failed {}", e),
            }
        }
        if let Some(match_sub1) = match_sub0.subcommand_matches("finish") {
            let br = match_sub1.value_of("hotfix_name").unwrap();
            match gf_subcmd("hotfix", "finish", "develop", br) {
                Ok(()) => println!("Run hotfix {} successfully", br),
                Err(e) => panic!("Run subcmd hotfix failed {}", e),
            }
        }
    }
}

fn main() {

    gf_run();

    /*
    let repo = Repository::open(".").unwrap();

    match
    //fastforward_merge_branch(&repo, "develop", "feature/f1")
    normal_merge_branch(&repo, "master", "develop", "merge develop test")
    //merge_branch(&repo, "master", "develop", false)
    {
        Ok(()) => println!("test success"),
        Err(e) => panic!("{}", e),
    }
    //let repo = Repository::init(".").unwrap();
    //create_initial_commit(&repo);
    */
}
