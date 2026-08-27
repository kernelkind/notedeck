use notedeck::{App, AppResponse, AppTool, RegisteredTool, ToolContext, ToolSpec};
use notedeck_chrome::NotedeckApp;

struct ToolProvidingApp;

impl App for ToolProvidingApp {
    fn render(&mut self, _ctx: &mut notedeck::AppContext<'_>, _ui: &mut egui::Ui) -> AppResponse {
        AppResponse::none()
    }

    fn tools(&self) -> Vec<RegisteredTool> {
        vec![RegisteredTool::new(TestTool)]
    }
}

struct TestTool;

#[derive(serde::Deserialize)]
struct TestArgs;

#[derive(serde::Serialize)]
struct TestOutput;

impl AppTool for TestTool {
    type Args = TestArgs;
    type Output = TestOutput;

    fn spec(&self) -> ToolSpec {
        ToolSpec::new("test_tool", "Test tool forwarding", Vec::new())
    }

    fn call(&self, _ctx: &mut ToolContext<'_>, _args: Self::Args) -> Result<Self::Output, String> {
        Ok(TestOutput)
    }
}

#[test]
fn notedeck_app_forwards_inner_app_tools() {
    let app = NotedeckApp::Other("test".to_owned(), Box::new(ToolProvidingApp));

    let names = app
        .tools()
        .into_iter()
        .map(|tool| tool.name())
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["test_tool"]);
}
