extern crate clap;
extern crate rpassword;

use std::str;
use std::string::String;
use std::path::Path;
use std::process::Command;
use std::fs::{self, File};
use std::io::{self, stdin, stdout, Read, Write};
use std::env;
use git2::*;
use clap::{Arg, App, SubCommand};

const RESET: &str = "\u{1b}[m";
const BOLD: &str = "\u{1b}[1m";
const RED: &str = "\u{1b}[31m";
const GREEN: &str = "\u{1b}[32m";
const CYAN: &str = "\u{1b}[36m";

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

fn tree_to_treeish<'a>(repo: &'a Repository, br_name: &str)
    -> Result<Option<Object<'a>>, Error> {
    let obj = repo.revparse_single(&br_name)?;
    let tree = obj.peel(ObjectType::Tree)?;
        Ok(Some(tree))
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

    Ok(())
}

fn normal_merge_branch(repo: &Repository, our_br: &str, their_br: &str) -> Result<(), Error> {
    let their_oid = repo.refname_to_id(&("refs/heads/".to_owned() + their_br))?;
    let their_commit = repo.find_commit(their_oid)?;
    let their_annotated_commit = repo.find_annotated_commit(their_oid)?;

    checkout_branch(&repo, our_br)?;
    repo.merge(&[&their_annotated_commit], None, None)?;
    let parent = find_last_commit(&repo)?;

    //git commit
    let sig = repo.signature()?;
    let tree_id = {
        let mut index = repo.index()?;

        index.write_tree()?
    };

    let tree = repo.find_tree(tree_id)?;

    let merge_msg: String;
    if our_br == "master" {
        merge_msg = "Bump to version ".to_owned();
    } else if our_br == "develop" {
        merge_msg = "Develop from version ".to_owned();
    } else {
        merge_msg = "Merge ".to_owned() + their_br + " to " + our_br;
    }
    let msg = edit_msg(".git/COMMIT_EDITMSG", &merge_msg);

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
    config_l.set_str("gitflow.prefix.bugfix", "bugfix/")?;
    config_l.set_str("gitflow.prefix.support", "support/")?;
    config_l.set_str("gitflow.prefix.versiontag", "")?;

    Ok(())
}

fn gf_config() {
    let repo = Repository::init(".").expect("Not a git-flow repository");
    let config_l = repo.config().expect("Can not get local cofniguration");

    let mut cfg = config_l.get_string("gitflow.branch.master").unwrap_or("".to_owned());
    println!("Branch name for production releases: {}", cfg);
    cfg = config_l.get_string("gitflow.branch.develop").unwrap_or("".to_owned());
    println!("Branch name for \"next release\" development: {}", cfg);
    cfg = config_l.get_string("gitflow.prefix.feature").unwrap_or("".to_owned());
    println!("Feature branch prefix: {}", cfg);
    cfg = config_l.get_string("gitflow.prefix.bugfix").unwrap_or("".to_owned());
    println!("Bugfix branch prefix: {}", cfg);
    cfg = config_l.get_string("gitflow.prefix.release").unwrap_or("".to_owned());
    println!("Release branch prefix: {}", cfg);
    cfg = config_l.get_string("gitflow.prefix.hotfix").unwrap_or("".to_owned());
    println!("Hotfix branch prefix: {}", cfg);
    cfg = config_l.get_string("gitflow.prefix.support").unwrap_or("".to_owned());
    println!("Support branch prefix: {}", cfg);
    cfg = config_l.get_string("gitflow.prefix.versiontag").unwrap_or("".to_owned());
    println!("Version tag prefix: {}", cfg);
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

fn edit_msg(path: &str, default_msg: &str) -> String {
    fs::write(path, default_msg).expect("Unable to write string");

    let editor = env::var("EDITOR").unwrap_or("nvim".into());
    Command::new(editor)
        .arg(path)
        .spawn()
        .expect("Spawn nvim failed")
        .wait()
        .expect("Waiting nvim failed");

    let mut msg = String::new();
    let mut f = File::open(path).expect("Unable to open file");
    f.read_to_string(&mut msg).expect("Unable to read string");

    msg
}

fn create_tag(repo: &Repository, br: &str) -> Result<Oid, Error> {
    let br_oid = repo.refname_to_id(&("refs/heads/".to_owned() + br))?;
    let br_obj = repo.find_object(br_oid, None)?;

    // get a user input tag
    let tagname = get_input("Input a tag name");

    let tag_msg = &("Release version ".to_owned() + &tagname);
    let msg = edit_msg(".git/TAG_EDITMSG", tag_msg);

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

fn gf_list_branch(gf_br: &str) {
    let p = &(".git/refs/heads/".to_owned() + gf_br);
    let gf_br_path = Path::new(p);
    if !gf_br_path.exists() {
        println!("No {} branches exist.", gf_br);
        return;
    }
    let paths = Path::read_dir(gf_br_path).unwrap();

    let mut cur_br = String::new();
    let mut f = File::open(".git/HEAD").expect("Unable to open file");
    f.read_to_string(&mut cur_br).expect("Unable to read string");
    cur_br = cur_br.replace(&("ref: refs/heads/".to_owned() + gf_br + "/"), "");

    for path in paths {
        let file_name = path.unwrap().file_name();
        let br = file_name.to_str().unwrap();
        if br == cur_br.trim() {
            println!("* {}", br);
        } else {
            println!("  {}", br);
        }
    }
}

fn gf_diff_branches(old: &str, new: Option<&str>) {
    let repo = Repository::open(".").expect("Not a git repository");
    let newtree: Object;
    let oldtree = tree_to_treeish(&repo, old)
        .expect("Get old tree failed")
        .unwrap();
    if new == None {
        let headref = repo.head().expect("Get head reference failed");
        let headname = headref.name().unwrap();
        newtree = tree_to_treeish(&repo, headname)
            .expect("Get old tree failed")
            .unwrap();
    } else {
        newtree = tree_to_treeish(&repo, new.unwrap())
            .expect("Get old tree failed")
            .unwrap();
    }

    let diff = repo.diff_tree_to_tree(Some(oldtree.as_tree().unwrap()), Some(newtree.as_tree().unwrap()), None)
        .expect("Get diff failed");

    let mut last_color = None;
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let next = match line.origin() {
            '+' => Some(GREEN),
            '-' => Some(RED),
            '>' => Some(GREEN),
            '<' => Some(RED),
            'F' => Some(BOLD),
            'H' => Some(CYAN),
            _ => None,
        };
        if next != last_color {
            if last_color == Some(BOLD) || next == Some(BOLD) {
                print!("{}", RESET);
            }
            print!("{}", next.unwrap_or(RESET));
            last_color = next;
        }

        match line.origin() {
            '+' | '-' | ' ' => print!("{}", line.origin()),
            _ => {}
        }
        print!("{}", str::from_utf8(line.content()).unwrap());
        true
    }).expect("Print diffs failed");
}

fn gf_publish(br_name: Option<&str>, user: &str, pass: &str) {
    // Urgly, TODO get remote name from repository?
    let remote_name = "origin";
    //let remote_branch = br_name.unwrap_or("master");
    let repo = Repository::open(".").expect("Not a git repository");
    let mut remote = repo.find_remote(remote_name).expect("Find remote name failed");

    let mut callbacks = RemoteCallbacks::new();
    /* Push */
    let mut options = PushOptions::new();

    callbacks.credentials(|_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext(user, pass)
    });
    //callbacks.push_update_reference(|refname, status| {
    //    Ok(())
    //});
    options.remote_callbacks(callbacks);
    // push the specified branch
    let br = "refs/heads/".to_owned() + br_name.unwrap_or("master");
    remote.push(&[&br], Some(&mut options)).unwrap();
}

fn gf_track(br_name: &str) -> Result<(), Error> {
    let repo = Repository::open(".").expect("Not a git repository");
    let remote = "origin";
    let mut cb = RemoteCallbacks::new();
    let mut remote = repo
        .find_remote(remote)
        .or_else(|_| repo.remote_anonymous(remote))?;
    cb.sideband_progress(|data| {
        print!("remote: {}", str::from_utf8(data).unwrap());
        io::stdout().flush().unwrap();
        true
    });

    // This callback gets called for each remote-tracking branch that gets
    // updated. The message we output depends on whether it's a new one or an
    // update.
    cb.update_tips(|refname, a, b| {
        if a.is_zero() {
            println!("[new]     {:20} {}", b, refname);
        } else {
            println!("[updated] {:10}..{:10} {}", a, b, refname);
        }
        true
    });

    // Here we show processed and total objects in the pack and the amount of
    // received data. Most frontends will probably want to show a percentage and
    // the download rate.
    cb.transfer_progress(|stats| {
        if stats.received_objects() == stats.total_objects() {
            print!(
                "Resolving deltas {}/{}\r",
                stats.indexed_deltas(),
                stats.total_deltas()
            );
        } else if stats.total_objects() > 0 {
            print!(
                "Received {}/{} objects ({}) in {} bytes\r",
                stats.received_objects(),
                stats.total_objects(),
                stats.indexed_objects(),
                stats.received_bytes()
            );
        }
        io::stdout().flush().unwrap();
        true
    });

    // Download the packfile and index it. This function updates the amount of
    // received data and the indexer stats which lets you inform the user about
    // progress.
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cb);
    remote.download(&[] as &[&str], Some(&mut fo))?;

    {
        // If there are local objects (we got a thin pack), then tell the user
        // how many objects we saved from having to cross the network.
        let stats = remote.stats();
        if stats.local_objects() > 0 {
            println!(
                "\rReceived {}/{} objects in {} bytes (used {} local \
                 objects)",
                stats.indexed_objects(),
                stats.total_objects(),
                stats.received_bytes(),
                stats.local_objects()
            );
        } else {
            println!(
                "\rReceived {}/{} objects in {} bytes",
                stats.indexed_objects(),
                stats.total_objects(),
                stats.received_bytes()
            );
        }
    }

    // Disconnect the underlying connection to prevent from idling.
    remote.disconnect();

    // Update the references in the remote's namespace to point to the right
    // commits. This may be needed even if there was no packfile to download,
    // which can happen e.g. when the branches have been changed but all the
    // needed objects are available locally.
    remote.update_tips(None, true, AutotagOption::Unspecified, None)?;

    checkout_branch(&repo, br_name)?;

    Ok(())
}

fn gf_rebase(cmd: &str, br_name: Option<&str>, opt: Option<&str>) {
    // git rebase develop [--interactive|--rebase-merges]

    let repo = Repository::open(".").expect("Not a git repository");

    let head_target = repo.head().unwrap().target().unwrap();
    let tip = repo.find_commit(head_target).unwrap();
    let sig = tip.author();

    if let Some(br_name) = br_name {
        let mut opts: RebaseOptions<'_> = Default::default();

        let bn = &("refs/heads/feature/".to_owned() + br_name);

        let head = repo.find_reference(bn).unwrap();
        let branch = repo.reference_to_annotated_commit(&head).unwrap();
        let develop = repo.find_reference("refs/heads/develop").unwrap();
        let upstream = repo.reference_to_annotated_commit(&develop).unwrap();
        let mut rebase = repo
            .rebase(Some(&branch), Some(&upstream), None, Some(&mut opts))
            .unwrap();

        while rebase.len() > 0 {
            rebase.next().unwrap().unwrap();
            let id = rebase.commit(None, &sig, None).unwrap();
            let commit = repo.find_commit(id).unwrap();
            println!("commit.message {}", commit.message().unwrap());
        }
        rebase.finish(None).unwrap();
    }
}

fn gf_run() {
    let matches = App::new("git-flow")
        .version("0.3.0")
        .author("Jason Wang <wang_borong@163.com>")
        .about("Workflow in git")
        // Init subcommand
        .subcommand(SubCommand::with_name("init")
            .about("Setup a git repository for git flow usage.")
            .arg(Arg::with_name("init_path")
                .help("Path to be initialized")))
        // Config subcommand
        .subcommand(SubCommand::with_name("config")
            .about("Show the git-flow configurations")
            )
        // Feature subcommand
        .subcommand(SubCommand::with_name("feature")
            .about("Manage your feature branches.")
            .subcommand(SubCommand::with_name("start")
                .about("Start new feature branch.")
                .arg(Arg::with_name("feature_name")
                    .help("The new feature to be started")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("finish")
                .about("Finish feature branch")
                .arg(Arg::with_name("feature_name")
                    .help("The feature to be finished")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("list")
                .about("Lists all the existing feature branches in the local repository"))
            .subcommand(SubCommand::with_name("publish")
                .about("Publish feature branch on origin.")
                .arg(Arg::with_name("feature_name")
                    .help("The feature to be published")))
            .subcommand(SubCommand::with_name("track")
                .about("Start tracking feature that is shared on origin")
                .arg(Arg::with_name("feature_name")
                    .help("The feature branch to be tracked")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("diff")
                .about("Show all changes in feature branch that are not in the base branch.")
                .arg(Arg::with_name("feature_name")
                    .help("The feature to be checked")))
            .subcommand(SubCommand::with_name("rebase")
                .about("Rebase feature on develop")
                .arg(Arg::with_name("interactive")
                    .short("i")
                    .help("Do an interactive rebase"))
                .arg(Arg::with_name("rebase-merges")
                    .short("r")
                    .help("Preserve merges"))
                .arg(Arg::with_name("feature_name")
                    .help("The feature branch to be rebased")
                    .index(1)))
            .subcommand(SubCommand::with_name("checkout")
                .about("Switch to feature branch")
                .arg(Arg::with_name("feature_name")
                    .help("The feature name to be checked out")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("delete")
                .about("Delete a given feature branch")
                .arg(Arg::with_name("feature_name")
                    .help("The feature branch to be deleted")
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
            .subcommand(SubCommand::with_name("list")
                .about("release list command"))
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
            .subcommand(SubCommand::with_name("list")
                .about("hotfix list command"))
        )
        .subcommand(SubCommand::with_name("bugfix")
            .about("git flow bugfix")
            .subcommand(SubCommand::with_name("start")
                .about("bugfix start command")
                .arg(Arg::with_name("bugfix_name")
                    .help("work on a bugfix branch")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("finish")
                .about("bugfix finish command")
                .arg(Arg::with_name("bugfix_name")
                    .help("work off a bugfix branch")
                    .required(true)
                    .index(1)))
            .subcommand(SubCommand::with_name("list")
                .about("bugfix list command"))
        )
        .subcommand(SubCommand::with_name("support")
            .about("git flow support")
            .subcommand(SubCommand::with_name("start")
                .about("support start command")
                .arg(Arg::with_name("bugfix_name")
                    .help("work on a bugfix branch")
                    .required(true)
                    .index(1))
                .arg(Arg::with_name("base_branch")
                    .help("the based branch which a support starts from")
                    .required(true)
                    .index(2)))
            .subcommand(SubCommand::with_name("list")
                .about("bugfix list command"))
        )
        // ...
        .get_matches();

    // Init
    if let Some(matches) = matches.subcommand_matches("init") {
        let path = matches.value_of("init_path").unwrap_or(".");
        match gf_init(&path) {
            Ok(()) => println!("Init {} Successfully", path),
            Err(_) => {
                println!("Init {} failed", path);
                return;
            },
        }
    }

    // Config
    if let Some(_matches) = matches.subcommand_matches("config") {
        gf_config();
    }

    // Feature
    if let Some(match_sub0) = matches.subcommand_matches("feature") {
        // start
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("feature_name").unwrap();
            match gf_subcmd("feature", "start", "develop", br) {
                Ok(()) => println!("Start feature {} successfully", br),
                Err(_) => {
                    println!("Start feature {} failed", br);
                    return;
                },
            }
        }
        // finish
        if let Some(match_sub1) = match_sub0.subcommand_matches("finish") {
            let br = match_sub1.value_of("feature_name").unwrap();
            match gf_subcmd("feature", "finish", "develop", br) {
                Ok(()) => println!("Run feature {} successfully", br),
                Err(_) => {
                    println!("Run feature {} failed", br);
                    return;
                },
            }
        }
        // list
        if let Some(_) = match_sub0.subcommand_matches("list") {
            gf_list_branch("feature");
        }
        // publish
        if let Some(match_sub1) = match_sub0.subcommand_matches("publish") {
            if match_sub1.is_present("feature_name") {
                let tmp_br = match_sub1.value_of("feature_name").unwrap();
                print!("User: ");
                let _ = stdout().flush();
                let mut user = String::new();
                stdin().read_line(&mut user).expect("Get user failed");
                let pass = rpassword::read_password_from_tty(Some("Password: ")).unwrap();
                gf_publish(Some(&("feature/".to_owned() + tmp_br)), &user, &pass);
            } else {
                gf_publish(None, "", "");
            }
        }
        // track
        if let Some(match_sub1) = match_sub0.subcommand_matches("track") {
            let br_name = match_sub1.value_of("feature_name")
                .expect("No feature name input");
            gf_track(&("feature/".to_owned() + br_name))
                .expect("Fetch remote failed");
        }
        // diff
        if let Some(match_sub1) = match_sub0.subcommand_matches("diff") {
            if match_sub1.is_present("feature_name") {
                let tmp_br = match_sub1.value_of("feature_name").unwrap();
                gf_diff_branches("develop", Some(&("feature/".to_owned() + tmp_br)));
            } else {
                gf_diff_branches("develop", None);
            }
        }
        // rebase
        if let Some(match_sub1) = match_sub0.subcommand_matches("rebase") {
            let mut opt = None;
            if match_sub1.is_present("interactive") {
                opt = Some("--interactive");
            } else if match_sub1.is_present("rebase-merges") {
                opt = Some("--rebase-merges");
            }

            let br_name = match_sub1.value_of("feature_name");
            gf_rebase(match_sub0.subcommand_name().unwrap(), br_name, opt);
        }
        // checkout
        if let Some(match_sub1) = match_sub0.subcommand_matches("checkout") {
            let br = match_sub1.value_of("feature_name").unwrap();
            let br_name = &("feature/".to_owned() + br);
            let repo = Repository::open(".").expect("Not a git repository");
            match checkout_branch(&repo, br_name) {
                Ok(()) => println!("Checkout to {} successfully", br_name),
                Err(_) => {
                    println!("Checkout to {} failed", br_name);
                    return;
                },
            }
        }
        //delete
        if let Some(match_sub1) = match_sub0.subcommand_matches("delete") {
            let br = match_sub1.value_of("feature_name").unwrap();
            let br_name = &("feature/".to_owned() + br);
            let repo = Repository::open(".").expect("Not a git repository");
            match delete_branch(&repo, br_name) {
                Ok(()) => println!("Delete {} successfully", br_name),
                Err(_) => {
                    println!("Delete {} failed", br_name);
                    return;
                },
            }
        }
    }

    // Release
    if let Some(match_sub0) = matches.subcommand_matches("release") {
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("release_name").unwrap();
            match gf_subcmd("release", "start", "develop", br) {
                Ok(()) => println!("Run release {} successfully", br),
                Err(_) => {
                    println!("Run release {} failed", br);
                    return;
                },
            }
        }
        if let Some(match_sub1) = match_sub0.subcommand_matches("finish") {
            let br = match_sub1.value_of("release_name").unwrap();
            match gf_subcmd("release", "finish", "develop", br) {
                Ok(()) => println!("Run release {} successfully", br),
                Err(_) => {
                    println!("Run release {} failed", br);
                    return;
                },
            }
        }
        if let Some(_) = match_sub0.subcommand_matches("list") {
            gf_list_branch("release");
        }
    }

    // Hotfix
    if let Some(match_sub0) = matches.subcommand_matches("hotfix") {
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("hotfix_name").unwrap();
            match gf_subcmd("hotfix", "start", "develop", br) {
                Ok(()) => println!("Run hotfix {} successfully", br),
                Err(_) => {
                    println!("Run hotfix {} failed", br);
                    return;
                },
            }
        }
        if let Some(match_sub1) = match_sub0.subcommand_matches("finish") {
            let br = match_sub1.value_of("hotfix_name").unwrap();
            match gf_subcmd("hotfix", "finish", "develop", br) {
                Ok(()) => println!("Run hotfix {} successfully", br),
                Err(_) => {
                    println!("Run hotfix {} failed", br);
                    return;
                },
            }
        }
        if let Some(_) = match_sub0.subcommand_matches("list") {
            gf_list_branch("hotfix");
        }
    }

    // Bugfix
    if let Some(match_sub0) = matches.subcommand_matches("bugfix") {
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("bugfix_name").unwrap();
            match gf_subcmd("bugfix", "start", "develop", br) {
                Ok(()) => println!("Run bugfix {} successfully", br),
                Err(_) => {
                    println!("Run bugfix {} failed", br);
                    return;
                },
            }
        }
        if let Some(match_sub1) = match_sub0.subcommand_matches("finish") {
            let br = match_sub1.value_of("bugfix_name").unwrap();
            match gf_subcmd("bugfix", "finish", "develop", br) {
                Ok(()) => println!("Run bugfix {} successfully", br),
                Err(_) => {
                    println!("Run bugfix {} failed", br);
                    return;
                },
            }
        }
        if let Some(_) = match_sub0.subcommand_matches("list") {
            gf_list_branch("bugfix");
        }
    }

    // Support
    if let Some(match_sub0) = matches.subcommand_matches("support") {
        if let Some(match_sub1) = match_sub0.subcommand_matches("start") {
            let br = match_sub1.value_of("support_name").unwrap();
            let base_br = match_sub1.value_of("base_branch").unwrap();
            match gf_subcmd("support", "start", base_br, br) {
                Ok(()) => println!("Run support {} successfully", br),
                Err(_) => {
                    println!("Run support {} failed", br);
                    return;
                },
            }
        }
        if let Some(_) = match_sub0.subcommand_matches("list") {
            gf_list_branch("support");
        }
    }
}

fn main() {
    gf_run();
}
