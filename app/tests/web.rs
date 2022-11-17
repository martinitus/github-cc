/// Run the tests with `wasm-pack test (--headless) --firefox `.
/// In non-headless mode, the browser developer tools will show the log output.

use wasm_bindgen_test::*;
use wasm_bindgen_test::wasm_bindgen_test_configure;
use gh_wasm::gh_api::GHClient;


// configure test runner to use headless browser
// See https://rustwasm.github.io/docs/wasm-bindgen/wasm-bindgen-test/usage.html for more information.
wasm_bindgen_test_configure!(run_in_browser);

// check that the whole test runner setup actually works, also setup console logging...
#[wasm_bindgen_test]
fn a_pass() {
    assert_eq!(1, 1);
    // we cannot call this in every test, and we need to call it in the first test...
    wasm_logger::init(wasm_logger::Config::default());
}

#[wasm_bindgen_test]
async fn test_get_repos_single_page() {
    let client = GHClient::new(None);
    let repos = client.get_user_repositories("maiksensi").await;
    assert!(repos.is_ok());
    // make sure below number matches with https://github.com/maiksensi
    assert_eq!(29, repos.unwrap().len());
}

#[wasm_bindgen_test]
async fn test_get_repos_multi_page() {
    let client = GHClient::new(None);
    let repos = client.get_user_repositories("jonashackt").await;
    assert!(repos.is_ok());
    // make sure below number matches with https://github.com/jonashackt
    // assert_eq!(145, repos.unwrap().len());
}
