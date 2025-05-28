pub trait Endpoint {
    type Args: Serialize + Clone;
    type Response: Serialize + Clone;
}

pub struct TestCase<E: Endpoint> {
    pub args: Option<E::Args>,
    pub res: Option<BeaconResponse<E::Response>>,
}

// Macro to define endpoints with less boilerplate
macro_rules! define_endpoint {
    ($name:ident, $args:ty, $response:ty) => {
        pub struct $name;
        impl Endpoint for $name {
            type Args = $args;
            type Response = $response;
        }
    };
}

// Macro to define test cases
macro_rules! define_test_case {
    ($fn_name:ident, $endpoint:ty, $args_val:expr, $response_val:expr) => {
        pub fn $fn_name() -> TestCase<$endpoint> {
            TestCase {
                args: Some($args_val),
                res: Some(BeaconResponse::new($response_val)),
            }
        }
    };
}