// pub fn run_generic_server_test<Es: 'static>(
//     config: ChainForkConfig,
//     get_client: impl Fn(&ChainForkConfig, Box<dyn IHttpClient>) -> Box<dyn ApiClientMethods<Es>>,
//     get_routes: impl Fn(&ChainForkConfig, MockApplicationMethods<Es>) -> FastifyRoutes<Es>,
//     test_cases: GenericServerTestCases<Es>
// ) {
//     let mock_api = get_mock_api(test_cases.clone());
    
//     let mut server: Option<FastifyInstance> = None;
//     let mut client: Option<Box<dyn ApiClientMethods<Es>>> = None;
//     let mut http_client: Option<HttpClient> = None;
    
//     before_all(|| {
//         async move {
//             let (server_instance, base_url) = get_test_server();
//             server = Some(server_instance);
            
//             let routes = get_routes(&config, mock_api.clone());
//             for route in routes {
//                 server.as_ref().unwrap().route(route);
//             }
            
//             let http = HttpClient::new(&base_url);
//             http_client = Some(http);
            
//             client = Some(get_client(&config, Box::new(http_client.as_ref().unwrap().clone())));
//         }
//     });
    
//     after_all(|| {
//         async move {
//             if let Some(server_instance) = server.take() {
//                 server_instance.close().await;
//             }
//         }
//     });
    
//     describe("run generic server tests", || {
//         for (key, _) in test_cases.iter() {
//             describe(&format!("{}", key), || {
//                 let wire_formats = vec![WireFormat::Json, WireFormat::Ssz];
                
//                 for format in wire_formats {
//                     it(&format!("{:?}", format), || {
//                         async move {
//                             let wire_format = format.clone();
//                             let local_init = ApiRequestInit {
//                                 request_wire_format: wire_format.clone(),
//                                 response_wire_format: wire_format,
//                             };
                            
//                             let route_id = key.clone();
//                             let test_case = &test_cases[&route_id];
                            
//                             let res = client.as_ref().unwrap().call(
//                                 route_id.clone(),
//                                 test_case.args.clone(),
//                                 local_init
//                             ).await;
                            
//                             expect(res.value()).to_equal(test_case.res.data);
//                             expect(res.meta()).to_equal(test_case.res.meta);
//                         }
//                     });
//                 }
//             });
//         }
//     });
// }