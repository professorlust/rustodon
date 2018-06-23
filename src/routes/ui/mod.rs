use chrono::offset::Utc;
use db::models::{Account, NewStatus, Status, User};
use db::{self, id_generator};
use error::Perhaps;
use failure::Error;
use rocket::request::{FlashMessage, Form};
use rocket::response::{NamedFile, Redirect};
use rocket::Route;
use std::path::{Path, PathBuf};

use GIT_REV;

mod auth;
mod templates;
mod view_helpers;

use self::templates::*;

pub fn routes() -> Vec<Route> {
    routes![
        index,
        user_page,
        user_page_paginated,
        settings_profile,
        settings_profile_update,
        status_page,
        create_status,
        auth::signin_get,
        auth::signin_post,
        auth::signout,
        auth::signup_get,
        auth::signup_post,
        static_files
    ]
}

#[derive(FromForm, Debug)]
pub struct UserPageParams {
    max_id: Option<i64>,
}

#[derive(Debug, FromForm)]
pub struct CreateStatusForm {
    content: String,
    content_warning: String,
}

#[post("/statuses/create", data = "<form>")]
pub fn create_status(
    user: User,
    db_conn: db::Connection,
    form: Form<CreateStatusForm>,
) -> Result<Redirect, Error> {
    let form_data = form.get();

    // convert CW to option if present, so we get proper nulls in DB
    let content_warning: Option<String> = if form_data.content_warning.len() > 0 {
        Some(form_data.content_warning.to_owned())
    } else {
        None
    };

    let _status = NewStatus {
        id: id_generator().next(),
        created_at: Utc::now(),
        text: form_data.content.to_owned(),
        content_warning: content_warning,
        account_id: user.account_id,
    }.insert(&db_conn)?;

    Ok(Redirect::to("/"))
}

#[get("/users/<username>/statuses/<status_id>", format = "text/html")]
pub fn status_page(
    username: String,
    status_id: u64,
    db_conn: db::Connection,
) -> Perhaps<StatusTemplate<'static>> {
    let account = try_resopt!(Account::fetch_local_by_username(&db_conn, username));
    let status = try_resopt!(Status::by_account_and_id(
        &db_conn,
        account.id,
        status_id as i64
    ));

    Ok(Some(StatusTemplate {
        status:  status,
        account: account,
        _parent: BaseTemplate {
            flash:    None,
            revision: GIT_REV,
        },
    }))
}

// This is due to [SergioBenitez/Rocket#376](https://github.com/SergioBenitez/Rocket/issues/376).
// If you don't like this, please complain over there.
#[get("/users/<username>", format = "text/html")]
pub fn user_page(
    username: String,
    db_conn: db::Connection,
    account: Option<Account>,
) -> Perhaps<UserTemplate<'static>> {
    user_page_paginated(username, UserPageParams { max_id: None }, db_conn, account)
}

#[get("/users/<username>?<params>", format = "text/html")]
pub fn user_page_paginated(
    username: String,
    params: UserPageParams,
    db_conn: db::Connection,
    account: Option<Account>,
) -> Perhaps<UserTemplate<'static>> {
    let account_to_show = try_resopt!(Account::fetch_local_by_username(&db_conn, username));
    let statuses: Vec<Status> = {
        let a = &account_to_show;
        a.statuses_before_id(&db_conn, params.max_id, 10)?
    };
    let prev_page_id = if let Some(prev_page_max_id) = statuses.iter().map(|s| s.id).min() {
        let bounds = account_to_show.status_id_bounds(&db_conn)?;
        // unwrap is safe since we already know we have statuses
        if prev_page_max_id > bounds.unwrap().0 {
            Some(prev_page_max_id)
        } else {
            None
        }
    } else {
        None
    };
    Ok(Some(UserTemplate {
        account_to_show: account_to_show,
        account: account,
        statuses: statuses,
        prev_page_id: prev_page_id,
        connection: db_conn,
        _parent: BaseTemplate {
            flash:    None,
            revision: GIT_REV,
        },
    }))
}

#[get("/settings/profile")]
pub fn settings_profile(
    db_conn: db::Connection,
    user: User,
) -> Perhaps<EditProfileTemplate<'static>> {
    Ok(Some(EditProfileTemplate {
        account: user.get_account(&db_conn)?,
        _parent: BaseTemplate {
            flash:    None,
            revision: GIT_REV,
        },
    }))
}

#[derive(Debug, FromForm)]
pub struct UpdateProfileForm {
    summary: String,
}

#[post("/settings/profile", data = "<form>")]
pub fn settings_profile_update(
    db_conn: db::Connection,
    user: User,
    form: Form<UpdateProfileForm>,
) -> Result<Redirect, Error> {
    let form_data = form.get();
    let account = user.get_account(&db_conn)?;

    // `as &str` defeat an incorrect deref coercion (due to the second match arm)
    let new_summary = match &form_data.summary as &str {
        "" => None,
        x => Some(x.to_string()),
    };
    account.set_summary(&db_conn, new_summary)?;

    Ok(Redirect::to("/settings/profile"))
}

#[get("/")]
pub fn index(flash: Option<FlashMessage>, account: Option<Account>) -> IndexTemplate<'static> {
    IndexTemplate {
        account: account,
        _parent: BaseTemplate {
            flash:    flash,
            revision: GIT_REV,
        },
    }
}

#[get("/static/<path..>")]
fn static_files(path: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(path)).ok()
}
