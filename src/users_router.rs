/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Contains the Iron router for user managing.
//!
//! # User Management Router
//!
//! The module contains the `UsersRouter` middleware in charge of managing
//! user-related REST operations. Exhaustive
//! [REST documentation](https://github.com/fxbox/users/blob/master/doc/API.md)
//! can be found in the GitHub repository.

use super::auth_middleware::{ AuthEndpoint, AuthMiddleware, SessionToken };
use super::users_db::{ User, UserBuilder, UsersDb, ReadFilter };
use super::errors::*;

use iron::status;
use iron::headers::{ Authorization, Basic };
use iron::method::Method;
use iron::prelude::*;
use iron_cors::CORS;
use router::Router;
use rustc_serialize::json;

use std::io::Read;

type Credentials = (String, String);

pub static API_VERSION: &'static str = "v1";

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct LoginResponse {
    session_token: String
}

impl LoginResponse {
    fn with_user(user: &User) -> IronResult<Response> {
        let session_token = match SessionToken::from_user(&user) {
            Ok(token) => token,
            Err(_) => return EndpointError::with(
                status::InternalServerError, 501, None
            )
        };
        let body_obj = LoginResponse{
           session_token: session_token
        };
        let body = match json::encode(&body_obj) {
            Ok(body) => body,
            Err(_) => return EndpointError::with(
                status::InternalServerError, 501, None
            )
        };
        Ok(Response::with((status::Created, body)))
    }
}

/// Manages user-related REST operations.
///
/// # Examples
///
/// To install the router, you use:
///
/// ```
/// extern crate iron;
/// extern crate foxbox_users;
///
/// fn main() {
///     use foxbox_users::UsersManager;
///     use iron::prelude::{Chain, Iron};
///
///     let manager = UsersManager::new("UsersRouter_0.sqlite");
///     let router = manager.get_router_chain();
///     let mut chain = Chain::new(router);
/// # if false {
///     Iron::new(chain).http("localhost:3000").unwrap();
/// # }
/// }
/// ```
pub struct UsersRouter;

impl UsersRouter {
    fn setup(req: &mut Request, db_path: &str) -> IronResult<Response> {
        #[derive(RustcDecodable, Debug)]
        struct SetupBody {
            username: String,
            email: String,
            password: String
        }

        // This endpoint should be disabled and return error 410 (Gone)
        // if there is any admin user already configured.
        let db = UsersDb::new(db_path);
        let admins = db.read(ReadFilter::IsAdmin(true)).unwrap();
        if !admins.is_empty() {
            return EndpointError::with(status::Gone, 410,
                Some("There is already an admin account".to_owned()));
        }

        let mut payload = String::new();
        req.body.read_to_string(&mut payload).unwrap();
        let body: SetupBody = match json::decode(&payload) {
            Ok(body) => body,
            Err(error) => {
                println!("{:?}", error);
                return from_decoder_error(error);
            }
        };

        let admin = match UserBuilder::new()
            .name(body.username)
            .email(body.email)
            .password(body.password)
            .admin(true)
            .finalize() {
                Ok(user) => user,
                Err(user_with_error) => {
                    println!("{:?}", user_with_error);
                    return from_user_builder_error(user_with_error.error);
                }
            };

        match db.create(&admin) {
            Ok(admin) => {
                LoginResponse::with_user(&admin)
            },
            Err(error) => {
                println!("{:?}", error);
                from_sqlite_error(error)
            }
        }
    }

    fn login(req: &mut Request, db_path: &str) -> IronResult<Response> {
        // Return Some pair of valid credentials if both username and password
        // are provided or None elsewhere.
        fn credentials_from_header(auth: &Authorization<Basic>)
            -> Option<Credentials> {
            let &Authorization(Basic {
                ref username,
                password: ref maybe_password
            }) = auth;
            let something_is_missed =
                username.is_empty() || match *maybe_password {
                    None => true,
                    Some(ref psw) => psw.is_empty()
                };
            if something_is_missed {
                None
            } else {
                Some((
                    username.to_owned(),
                    maybe_password.as_ref().unwrap().to_owned()
                ))
            }
        }

        let error103 = EndpointError::with(status::BadRequest, 103,
            Some("Missing or malformed authentication header".to_owned()));
        let header: Option<&Authorization<Basic>> = req.headers.get();
        if let Some(auth) = header {
            if let Some((username, password)) = credentials_from_header(auth) {
                let users_db = UsersDb::new(db_path);
                let users = match users_db.read(
                    ReadFilter::Credentials(username, password)) {
                    Ok(users) => users,
                    Err(_) => return EndpointError::with(
                        status::InternalServerError, 501, None
                    )
                };
                if users.len() != 1 {
                    return EndpointError::with(status::Unauthorized, 401, None);
                }
                LoginResponse::with_user(&users[0])
            } else {
                error103
            }
        } else {
            error103
        }
    }

    pub fn create_user(req: &mut Request, db_path: &str)
        -> IronResult<Response> {
        EndpointError::with(status::NotFound, 404, None)
    }

    pub fn get_user(req: &mut Request, db_path: &str)
        -> IronResult<Response> {
        EndpointError::with(status::NotFound, 404, None)
    }

    pub fn get_all_users(req: &mut Request, db_path: &str)
        -> IronResult<Response> {
        EndpointError::with(status::NotFound, 404, None)
    }

    pub fn edit_user(req: &mut Request, db_path: &str)
        -> IronResult<Response> {
        EndpointError::with(status::NotFound, 404, None)
    }

    pub fn delete_user(req: &mut Request, db_path: &str)
        -> IronResult<Response> {
        EndpointError::with(status::NotFound, 404, None)
    }

    /// Creates the Iron user router middleware.
    pub fn init(db_path: &str) -> super::iron::middleware::Chain {
        let mut router = Router::new();

        // Setup.
        let data = String::from(db_path);
        router.post(format!("/{}/setup", API_VERSION),
                    move |req: &mut Request| -> IronResult<Response> {
            UsersRouter::setup(req, &data)
        });

        // Login.
        let data = String::from(db_path);
        router.post(format!("/{}/login", API_VERSION),
                    move |req: &mut Request| -> IronResult<Response> {
            UsersRouter::login(req, &data)
        });

        // User management.
        let data = String::from(db_path);
        router.post(format!("/{}/users", API_VERSION),
                    move |req: &mut Request| -> IronResult<Response> {
            UsersRouter::create_user(req, &data)
        });

        let data = String::from(db_path);
        router.get(format!("/{}/users/:id", API_VERSION),
                   move |req: &mut Request| -> IronResult<Response> {
            UsersRouter::get_user(req, &data)
        });

        let data = String::from(db_path);
        router.get(format!("/{}/users", API_VERSION),
                   move |req: &mut Request| -> IronResult<Response> {
            UsersRouter::get_all_users(req, &data)
        });

        let data = String::from(db_path);
        router.put(format!("/{}/users/:id", API_VERSION),
                   move |req: &mut Request| -> IronResult<Response> {
            UsersRouter::edit_user(req, &data)
        });

        let data = String::from(db_path);
        router.delete(format!("/{}/users/:id", API_VERSION),
                      move |req: &mut Request| -> IronResult<Response> {
            UsersRouter::delete_user(req, &data)
        });

        let cors = CORS::new(vec![
            (vec![Method::Post],
             format!("/{}/login", API_VERSION)),
            (vec![Method::Post, Method::Get],
             format!("/{}/users", API_VERSION)),
            (vec![Method::Get, Method::Put, Method::Delete],
             format!("/{}/users/:id", API_VERSION))
        ]);

        let data = String::from(db_path);
        let auth_middleware = AuthMiddleware::new(vec![
            AuthEndpoint(vec![Method::Post, Method::Get],
                         format!("/{}/users", API_VERSION)),
            AuthEndpoint(vec![Method::Put, Method::Delete],
                         format!("/{}/users/:id", API_VERSION))
        ], data);

        let mut chain = Chain::new(router);
        chain.link_after(cors);
        chain.link_around(auth_middleware);

        chain
    }
}

#[cfg(test)]
describe! cors_tests {
    before_each {
        use iron::{ headers, Headers };
        use iron_test::request;
        use super::super::users_db::get_db_environment;
        use super::super::UsersManager;
        use super::API_VERSION;

        let manager = UsersManager::new(&get_db_environment());
        let router = manager.get_router_chain();
    }

    it "should get the appropriate CORS headers" {
        use iron::method::Method;

        let endpoints = vec![
            (vec![Method::Post], format!("{}/login", API_VERSION))
        ];
        for endpoint in endpoints {
            let (_, path) = endpoint;
            let path = format!("http://localhost:3000/{}",
                               &(path.replace(":", "foo")));
            match request::options(&path, Headers::new(), &router) {
                Ok(res) => {
                    let headers = &res.headers;
                    assert!(headers.has::<headers::AccessControlAllowOrigin>());
                    assert!(headers.has::<headers::AccessControlAllowHeaders>());
                    assert!(headers.has::<headers::AccessControlAllowMethods>());
                },
                _ => {
                    assert!(false)
                }
            }
        }
    }

    it "should get the appropriate CORS headers even in case of error" {
        match request::post(&format!("http://localhost:3000/{}/login", API_VERSION),
                            Headers::new(),
                            "{}",
                            &router) {
            Ok(_) => {
                assert!(false)
            },
            Err(err) => {
                let headers = &err.response.headers;
                assert!(headers.has::<headers::AccessControlAllowOrigin>());
                assert!(headers.has::<headers::AccessControlAllowHeaders>());
                assert!(headers.has::<headers::AccessControlAllowMethods>());
            }

        }
    }

    it "should not get CORS headers" {
        match request::options(&format!("http://localhost:3000/{}/setup", API_VERSION),
                               Headers::new(),
                               &router) {
            Ok(res) => {
                let headers = &res.headers;
                assert!(!headers.has::<headers::AccessControlAllowOrigin>());
                assert!(!headers.has::<headers::AccessControlAllowHeaders>());
                assert!(!headers.has::<headers::AccessControlAllowMethods>());
            },
            _ => {
                assert!(false)
            }
        }
    }
}

#[cfg(test)]
describe! setup_tests {
    before_each {
        use iron::Headers;
        use iron::status::Status;
        use iron_test::request;
        use super::super::users_db::{ get_db_environment, remove_test_db };
        use super::super::UsersManager;

        let manager = UsersManager::new(&get_db_environment());
        let router = manager.get_router_chain();
        let usersDb = manager.get_db();
        usersDb.clear().ok();

        let endpoint = &format!("http://localhost:3000/{}/setup", API_VERSION);
    }

    it "should respond 201 Created for a proper POST /setup" {
        use super::LoginResponse;
        use super::super::auth_middleware::SessionClaims;
        use iron::prelude::Response;
        use iron_test::response::extract_body_to_string;
        use jwt;
        use rustc_serialize::Decodable;
        use rustc_serialize::json::{ self, DecodeResult };

        fn extract_body_to<T: Decodable>(response: Response) -> DecodeResult<T> {
            json::decode(&extract_body_to_string(response))
        }

        match request::post(endpoint, Headers::new(),
                            "{\"username\": \"username\",
                              \"email\": \"username@domain.com\",
                              \"password\": \"password\"}",
                            &router) {
            Ok(res) => {
                assert_eq!(res.status.unwrap(), Status::Created);
                let body_obj = extract_body_to::<LoginResponse>(res).unwrap();
                let token = body_obj.session_token;
                let claims = jwt::Token::<jwt::Header, SessionClaims>::parse(&token)
                    .ok().unwrap().claims;
                assert_eq!(claims.name, "username");
            },
            Err(err) => {
                println!("{:?}", err);
                assert!(false);
            }
        };
    }

    it "should create one admin user" {
        use super::super::users_db::ReadFilter;

        let body = "{\"username\": \"username\",\
                    \"email\": \"username@domain.com\",\
                    \"password\": \"password\"}";

        if let Ok(res) = request::post(endpoint, Headers::new(), body, &router) {
            assert_eq!(res.status.unwrap(), Status::Created);
            let admins = usersDb.read(ReadFilter::IsAdmin(true)).unwrap();
            assert_eq!(admins.len(), 1);
            assert_eq!(admins[0].email, "username@domain.com");
        } else {
            assert!(false);
        };
    }

    it "should respond 410 Gone if an admin account exists" {
        use iron::prelude::Response;
        use rustc_serialize::Decodable;
        use rustc_serialize::json::{self, DecodeResult};
        fn extract_body_to<T: Decodable>(response: Response) -> DecodeResult<T> {
            use iron_test::response::extract_body_to_string;
            json::decode(&extract_body_to_string(response))
        }

        use super::super::errors::{ErrorBody};

        // Be sure we have an admin
        use super::super::users_db::UserBuilder;
        usersDb.create(&UserBuilder::new()
                   .id(1).name(String::from("admin"))
                   .password(String::from("password!!"))
                   .email(String::from("admin@example.com"))
                   .admin(true)
                   .finalize().unwrap()).ok();
        match request::post(endpoint, Headers::new(),
                            "{\"username\": \"u\",
                              \"email\": \"u@d\",
                              \"password\": \"12345678\"}",
                            &router) {
            Ok(_) => {
                assert!(false);
            },
            Err(error) => {
                let response = error.response;
                assert!(response.status.is_some());
                assert_eq!(response.status.unwrap(), Status::Gone);
                let json = extract_body_to::<ErrorBody>(response).unwrap();
                assert_eq!(json.errno, 410);
                assert_eq!(json.message, Some("There is already an admin account".to_owned()));
            }
        };
    }

    it "should respond 400 BadRequest, errno 100 if username is missing" {
        use iron::prelude::Response;
        use rustc_serialize::Decodable;
        use rustc_serialize::json::{self, DecodeResult};
        fn extract_body_to<T: Decodable>(response: Response) -> DecodeResult<T> {
            use iron_test::response::extract_body_to_string;
            json::decode(&extract_body_to_string(response))
        }

        use super::super::errors::{ErrorBody};

        match request::post(endpoint, Headers::new(),
                            "{\"email\": \"u@d\",
                              \"password\": \"12345678\"}",
                            &router) {
            Ok(_) => {
                assert!(false);
            },
            Err(error) => {
                let response = error.response;
                assert!(response.status.is_some());
                assert_eq!(response.status.unwrap(), Status::BadRequest);
                let json = extract_body_to::<ErrorBody>(response).unwrap();
                assert_eq!(json.errno, 100);
                assert_eq!(json.message, Some("Invalid user name".to_owned()));
            }
        };
    }

    it "should respond 400 BadRequest, errno 101 if email is missing" {
        use iron::prelude::Response;
        use rustc_serialize::Decodable;
        use rustc_serialize::json::{self, DecodeResult};
        fn extract_body_to<T: Decodable>(response: Response) -> DecodeResult<T> {
            use iron_test::response::extract_body_to_string;
            json::decode(&extract_body_to_string(response))
        }

        use super::super::errors::{ErrorBody};

        match request::post(endpoint, Headers::new(),
                            "{\"username\": \"u\",
                              \"password\": \"12345678\"}",
                            &router) {
            Ok(_) => {
                assert!(false);
            },
            Err(error) => {
                let response = error.response;
                assert!(response.status.is_some());
                assert_eq!(response.status.unwrap(), Status::BadRequest);
                let json = extract_body_to::<ErrorBody>(response).unwrap();
                assert_eq!(json.errno, 101);
                assert_eq!(json.message, Some("Invalid email".to_owned()));
            }
        };
    }

    it "should respond 400 BadRequest, errno 102 if password is missing" {
        use iron::prelude::Response;
        use rustc_serialize::Decodable;
        use rustc_serialize::json::{self, DecodeResult};
        fn extract_body_to<T: Decodable>(response: Response) -> DecodeResult<T> {
            use iron_test::response::extract_body_to_string;
            json::decode(&extract_body_to_string(response))
        }

        use super::super::errors::{ErrorBody};

        match request::post(endpoint, Headers::new(),
                            "{\"username\": \"u\",
                              \"email\": \"u@d\"}",
                            &router) {
            Ok(_) => {
                assert!(false);
            },
            Err(error) => {
                let response = error.response;
                assert!(response.status.is_some());
                assert_eq!(response.status.unwrap(), Status::BadRequest);
                let json = extract_body_to::<ErrorBody>(response).unwrap();
                assert_eq!(json.errno, 102);
                assert_eq!(json.message,
                    Some("Invalid password. Passwords must have a minimum of 8 chars".to_owned()));
            }
        };
    }

    after_each {
        remove_test_db();
    }
}

#[cfg(test)]
describe! login_tests {
    before_each {
        use super::super::users_db::{UserBuilder,
                                     remove_test_db,
                                     get_db_environment};
        use super::super::UsersManager;
        use iron::prelude::Response;
        use iron::Headers;
        #[allow(unused_imports)]
        use iron::headers::{Authorization, Basic};
        use iron::status::Status;
        use iron_test::request;
        use iron_test::response::extract_body_to_string;
        use rustc_serialize::Decodable;
        use rustc_serialize::json::{self, DecodeResult};
        #[allow(unused_imports)]
        use super::super::errors::{ErrorBody};

        #[allow(dead_code)]
        fn extract_body_to<T: Decodable>(response: Response) -> DecodeResult<T> {
            json::decode(&extract_body_to_string(response))
        }

        let manager = UsersManager::new(&get_db_environment());
        let router = manager.get_router_chain();
        let usersDb = manager.get_db();
        usersDb.clear().ok();
        usersDb.create(&UserBuilder::new()
                   .id(1).name(String::from("username"))
                   .password(String::from("password"))
                   .email(String::from("username@example.com"))
                   .secret(String::from("secret"))
                   .finalize().unwrap()).ok();
        let endpoint = &format!("http://localhost:3000/{}/login", API_VERSION);
    }

    it "should respond with a generic 400 Bad Request for requests missing username" {
        let invalid_credentials = Authorization(Basic {
            username: "".to_owned(),
            password: Some("password".to_owned())
        });
        let mut headers = Headers::new();
        headers.set(invalid_credentials);

        if let Err(error) = request::post(endpoint, headers, "", &router) {
            let response = error.response;
            assert!(response.status.is_some());
            assert_eq!(response.status.unwrap(), Status::BadRequest);
            let json = extract_body_to::<ErrorBody>(response).unwrap();
            assert_eq!(json.errno, 103);
        } else {
            assert!(false);
        };
    }

    it "should respond with a generic 400 Bad Request for requests missing password" {
        let invalid_credentials = Authorization(Basic {
            username: "username".to_owned(),
            password: Some("".to_owned())
        });
        let mut headers = Headers::new();
        headers.set(invalid_credentials);

        if let Err(error) = request::post(endpoint, headers, "", &router) {
            let response = error.response;
            assert!(response.status.is_some());
            assert_eq!(response.status.unwrap(), Status::BadRequest);
            let json = extract_body_to::<ErrorBody>(response).unwrap();
            assert_eq!(json.errno, 103);
        } else {
            assert!(false);
        };
    }

    it "should respond with a 400 Bad Request for requests missing the authorization password" {
        let headers = Headers::new();

        if let Err(error) = request::post(endpoint, headers, "", &router) {
            let response = error.response;
            assert!(response.status.is_some());
            assert_eq!(response.status.unwrap(), Status::BadRequest);
            let json = extract_body_to::<ErrorBody>(response).unwrap();
            assert_eq!(json.errno, 103);
        } else {
            assert!(false);
        };
    }

    it "should respond with a 401 Unauthorized for invalid credentials" {
        let invalid_credentials = Authorization(Basic {
            username: "johndoe".to_owned(),
            password: Some("password".to_owned())
        });
        let mut headers = Headers::new();
        headers.set(invalid_credentials);

        if let Err(error) = request::post(endpoint, headers, "", &router) {
            let response = error.response;
            assert!(response.status.is_some());
            assert_eq!(response.status.unwrap(), Status::Unauthorized);
        } else {
            assert!(false);
        };
    }

    it "should respond with a 201 Created and a valid JWT token in body for valid credentials" {
        use jwt;
        use super::LoginResponse;
        use super::super::auth_middleware::SessionClaims;

        let valid_credentials = Authorization(Basic {
            username: "username".to_owned(),
            password: Some("password".to_owned())
        });
        let mut headers = Headers::new();
        headers.set(valid_credentials);

        if let Ok(response) = request::post(endpoint, headers, "", &router) {
            assert!(response.status.is_some());
            assert_eq!(response.status.unwrap(), Status::Created);
            let body_obj = extract_body_to::<LoginResponse>(response).unwrap();
            let token = body_obj.session_token;
            let claims = jwt::Token::<jwt::Header, SessionClaims>::parse(&token).ok().unwrap().claims;
            assert_eq!(claims.id, 1);
            assert_eq!(claims.name, "username");
        } else {
            assert!(false);
        };
    }

    after_each {
        remove_test_db();
    }
}
