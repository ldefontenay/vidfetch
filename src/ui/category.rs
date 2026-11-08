//! Categories, as in NoteFetch (log entry 3, Q6 and Q7): real folders in
//! the output folder, one for each category and Uncategorised. The Category
//! to show box picks one, or all of them; new downloads go into the one
//! shown, or Uncategorised when all are shown. The Category menu makes,
//! renames and deletes them, and Ctrl+Shift+M moves a download between them.

use super::*;
use crate::places;

/// The first item of the box: every category.
const ALL: &str = "All categories";

/// What the box lists: all, Uncategorised, then the user's categories.
fn choices(app: &App) -> Vec<String> {
    [ALL.to_string(), library::UNCATEGORISED.to_string()]
        .into_iter()
        .chain(library::categories(&app.cfg.output_folder()))
        .collect()
}

/// Fills the Category to show box, the category shown chosen. A category
/// gone from the output folder is no longer shown.
pub fn fill(hwnd: HWND, app: &mut App) {
    let items = choices(app);
    let at = match app.cfg.category.as_deref() {
        None => Some(0),
        Some(c) => items
            .iter()
            .skip(1)
            .position(|x| x.eq_ignore_ascii_case(c))
            .map(|i| i + 1),
    };
    if at.is_none() {
        app.cfg.category = None;
    }
    unsafe {
        let Ok(combo) = GetDlgItem(Some(hwnd), IDC_CATEGORY) else {
            return;
        };
        SendMessageW(combo, CB_RESETCONTENT, None, None);
        for item in &items {
            let t = wide(item);
            SendMessageW(combo, CB_ADDSTRING, None, Some(LPARAM(t.as_ptr() as isize)));
        }
        SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(at.unwrap_or(0))), None);
    }
}

/// The category the box shows, as said: "all categories" or its name.
pub fn shown(app: &App) -> String {
    app.cfg
        .category
        .clone()
        .unwrap_or_else(|| "all categories".into())
}

/// A choice made in the Category to show box.
pub fn chosen(hwnd: HWND, app: &mut App) {
    let at = unsafe {
        GetDlgItem(Some(hwnd), IDC_CATEGORY)
            .map(|c| SendMessageW(c, CB_GETCURSEL, None, None).0)
            .unwrap_or(0)
    };
    let items = choices(app);
    let Some(item) = usize::try_from(at).ok().and_then(|i| items.get(i)) else {
        return;
    };
    app.cfg.category = (at != 0).then(|| item.clone());
    app.cfg.save();
    // Back to the downloads, from wherever the list was.
    app.online.clear();
    app.inside = None;
    name_list(hwnd, app);
    refresh(hwnd, app);
    let n = count(app.entries.len());
    let said = match &app.cfg.category {
        None => format!("Showing all categories: {n}. New downloads go into Uncategorised."),
        Some(c) => format!("Showing {c}: {n}. New downloads go into {c}."),
    };
    say(hwnd, &said);
}

/// Something running or waiting writes into `folder`.
fn in_use(app: &App, folder: &Path) -> bool {
    app.current.iter().chain(app.queue.iter()).any(|q| {
        q.settings.output_folder.starts_with(folder)
            || q.into.as_deref().is_some_and(|i| i.starts_with(folder))
    })
}

fn record_moved(from: &Path, to: &Path) {
    if let Some(r) = Config::past_path() {
        library::moved_folder(&r, from, to);
    }
    places::moved(from, to);
}

/// File, New category: asks for its name and makes its folder.
pub fn new_category(hwnd: HWND, app: &mut App) -> Option<String> {
    let output = app.cfg.output_folder();
    let name = crate::namebox::ask(
        hwnd,
        "New category - VidFetch",
        "&Name of the new category, such as Talks:",
        "",
        library::bad_name,
    )?;
    let name = name.trim().to_string();
    if let Some(why) = library::bad_category(&output, &name) {
        message::problem(hwnd, &why);
        focus(hwnd, IDC_LIST);
        return None;
    }
    if let Err(e) = std::fs::create_dir_all(output.join(&name)) {
        message::problem(hwnd, &format!("The category {name} could not be made: {e}"));
        return None;
    }
    fill(hwnd, app);
    Some(name)
}

/// Category, Rename the category shown.
pub fn rename(hwnd: HWND, app: &mut App) {
    let Some(old) = app
        .cfg
        .category
        .clone()
        .filter(|c| !library::means_uncategorised(c))
    else {
        say(
            hwnd,
            "Choose one of your categories in the Category to show box first (Alt+C). \
             Uncategorised is VidFetch's own, and cannot be renamed.",
        );
        return;
    };
    let output = app.cfg.output_folder();
    let from = output.join(&old);
    if in_use(app, &from) {
        say(
            hwnd,
            &format!(
                "A download is going into {old}, now or from the queue. Rename it once that is done."
            ),
        );
        return;
    }
    let Some(new) = crate::namebox::ask(
        hwnd,
        "Rename the category - VidFetch",
        "&New name for the category:",
        &old,
        library::bad_name,
    ) else {
        say(hwnd, "Nothing renamed.");
        focus(hwnd, IDC_LIST);
        return;
    };
    let new = new.trim().to_string();
    if new == old {
        say(hwnd, "The name is the same, so nothing changed.");
        return;
    }
    let same = new.eq_ignore_ascii_case(&old);
    if let Some(why) = library::bad_category(&output, &new).filter(|_| !same) {
        message::problem(hwnd, &why);
        focus(hwnd, IDC_LIST);
        return;
    }
    let to = output.join(&new);
    if let Err(e) = std::fs::rename(&from, &to) {
        message::problem(
            hwnd,
            &format!(
                "{old} could not be renamed: {e}. A file in it may be open in another program, \
                 such as a media player."
            ),
        );
        return;
    }
    record_moved(&from, &to);
    app.cfg.category = Some(new.clone());
    app.cfg.save();
    fill(hwnd, app);
    refresh(hwnd, app);
    say(hwnd, &format!("Renamed the category {old} to {new}."));
    focus(hwnd, IDC_LIST);
}

/// Category, Delete the category shown: what is in it goes into
/// Uncategorised; nothing downloaded is deleted.
pub fn delete(hwnd: HWND, app: &mut App) {
    let Some(name) = app
        .cfg
        .category
        .clone()
        .filter(|c| !library::means_uncategorised(c))
    else {
        say(
            hwnd,
            "Choose one of your categories in the Category to show box first (Alt+C). \
             Uncategorised is VidFetch's own, and cannot be deleted.",
        );
        return;
    };
    let output = app.cfg.output_folder();
    let folder = output.join(&name);
    if in_use(app, &folder) {
        say(
            hwnd,
            &format!(
                "A download is going into {name}, now or from the queue. Delete it once that is done."
            ),
        );
        return;
    }
    let inside = library::list(&folder);
    let what = match inside.len() {
        0 => format!("Delete the category {name}? It is empty."),
        n => format!(
            "Delete the category {name}? Its {} move into Uncategorised; nothing is deleted but \
             the category.",
            count(n)
        ),
    };
    if !message::ask(hwnd, &what, "&Delete the category", "&Keep it", false) {
        say(hwnd, "The category was kept.");
        focus(hwnd, IDC_LIST);
        return;
    }
    let into = output.join(library::UNCATEGORISED);
    let mut problems = Vec::new();
    for e in &inside {
        match library::move_to(e, &into) {
            Ok(to) => record_moved(&e.path, &to),
            Err(why) => problems.push(why),
        }
    }
    let gone = std::fs::remove_dir(&folder).is_ok();
    app.cfg.category = None;
    app.cfg.save();
    fill(hwnd, app);
    app.inside = None;
    app.online.clear();
    name_list(hwnd, app);
    refresh(hwnd, app);
    let mut said = format!("Deleted the category {name}.");
    if !inside.is_empty() {
        said += " Its downloads are in Uncategorised. Showing all categories.";
    }
    if !gone {
        said += " Its folder still holds files VidFetch does not list, so it was left in place.";
    }
    say(hwnd, &said);
    if !problems.is_empty() {
        message::problem(hwnd, &problems.join("\n\n"));
    }
    focus(hwnd, IDC_LIST);
}

const IDM_TO: i32 = 2400;
const IDM_TO_NEW: i32 = 2399;

/// Ctrl+Shift+M: moves the download chosen into another category, chosen
/// from a menu.
pub fn move_chosen(hwnd: HWND, app: &mut App) {
    if app.inside.is_some() {
        say(
            hwnd,
            "A playlist's videos move with it: Escape goes back to the playlist, then Ctrl+Shift+M \
             moves it.",
        );
        return;
    }
    let Some(e) = super::chosen(hwnd, app) else {
        return;
    };
    if busy_with(hwnd, app, &e) {
        return;
    }
    let output = app.cfg.output_folder();
    let targets: Vec<String> = std::iter::once(library::UNCATEGORISED.to_string())
        .chain(library::categories(&output))
        .filter(|c| !c.eq_ignore_ascii_case(&e.category))
        .collect();
    let mut items: Vec<(i32, String)> = targets
        .iter()
        .enumerate()
        .map(|(i, c)| (IDM_TO + i as i32, c.clone()))
        .collect();
    items.push((0, String::new()));
    items.push((IDM_TO_NEW, "&New category...".into()));
    let refs: Vec<(i32, &str)> = items.iter().map(|(i, t)| (*i, t.as_str())).collect();
    let Some(id) = downloads::context_menu(hwnd, list(hwnd), None, &refs) else {
        return;
    };
    let target = if id == IDM_TO_NEW {
        match new_category(hwnd, app) {
            Some(n) => n,
            None => {
                say(hwnd, "Nothing moved.");
                focus(hwnd, IDC_LIST);
                return;
            }
        }
    } else {
        match targets.get((id - IDM_TO) as usize) {
            Some(t) => t.clone(),
            None => return,
        }
    };
    match library::move_to(&e, &output.join(&target)) {
        Ok(to) => {
            record_moved(&e.path, &to);
            app.select = Some(to);
            refresh(hwnd, app);
            say(hwnd, &format!("Moved {} to {target}.", e.name));
        }
        Err(why) => message::problem(hwnd, &why),
    }
    focus(hwnd, IDC_LIST);
}

/// Moves what lies directly in the output folder into Uncategorised, the
/// first time categories are used or after File Explorer put it there. How
/// many were moved; the queue's records follow them.
pub fn tidy(app: &mut App) -> usize {
    let output = app.cfg.output_folder();
    let moved = library::tidy(&output, Config::past_path().as_deref());
    for (from, to) in &moved {
        places::moved(from, to);
        for q in app.resume.iter_mut() {
            if q.into.as_deref() == Some(from.as_path()) {
                q.into = Some(to.clone());
            }
        }
    }
    // What waits from before categories goes into Uncategorised too.
    let un = output.join(library::UNCATEGORISED);
    for q in app.resume.iter_mut() {
        if q.settings.output_folder == output {
            q.settings.output_folder = un.clone();
        }
    }
    moved.len()
}
