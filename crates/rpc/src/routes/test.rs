use actix_web::{HttpRequest, get};

#[get("/welcome")]
async fn welcome(req: HttpRequest) -> &'static str {
    println!("REQ: {req:?}");
    "Hello world!"
}
