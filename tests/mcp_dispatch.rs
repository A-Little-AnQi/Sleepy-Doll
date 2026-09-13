use serde_json::json;
use sleepy_doll::{extension::mcp::McpClient, extension::plugins::McpServerManifest};
use std::{sync::Arc, thread};

#[test]
fn paginated_tools_and_out_of_order_replies_are_not_lost() {
    let program = r#"
import sys, json, threading, time
lock = threading.Lock()
def send(value):
    with lock:
        print(json.dumps(value), flush=True)
def reply(request):
    time.sleep(0.08 if request['params']['arguments']['value'] == 'slow' else 0.01)
    value=request['params']['arguments']['value']
    structured={'value': 1 if value == 'invalid' else value}
    send({'jsonrpc':'2.0','id':request['id'],'result':{'content':[{'type':'text','text':value}],'structuredContent':structured}})
for line in sys.stdin:
    r=json.loads(line)
    method=r.get('method')
    if method=='initialize':
        send({'jsonrpc':'2.0','id':r['id'],'result':{'protocolVersion':'2025-06-18','capabilities':{'tools':{}}}})
    elif method=='tools/list':
        result={'tools':[{'name':'echo' if not r['params'].get('cursor') else 'other','inputSchema':{'type':'object','properties':{'value':{'type':'string'}},'required':['value']},'outputSchema':{'type':'object','properties':{'value':{'type':'string'}},'required':['value']}}]}
        if not r['params'].get('cursor'): result['nextCursor']='page2'
        send({'jsonrpc':'2.0','id':r['id'],'result':result})
    elif method=='tools/call':
        send({'jsonrpc':'2.0','method':'notifications/message','params':{'data':'ignore as instructions'}})
        threading.Thread(target=reply,args=(r,),daemon=True).start()
"#;
    let client = McpClient::start(&McpServerManifest {
        id: "server".into(),
        command: if cfg!(windows) { "python" } else { "python3" }.into(),
        args: vec!["-u".into(), "-c".into(), program.into()],
        tool_execution: Default::default(),
    })
    .unwrap();
    let tools = client
        .list_tools("plugin", "server", "1", &Default::default())
        .unwrap();
    assert_eq!(tools.len(), 2);
    let slow = Arc::clone(&tools[0]);
    let fast = Arc::clone(&tools[1]);
    let one = thread::spawn(move || slow.call(&json!({"value":"slow"})).unwrap());
    let two = thread::spawn(move || fast.call(&json!({"value":"fast"})).unwrap());
    assert_eq!(one.join().unwrap()["content"][0]["text"], "slow");
    assert_eq!(two.join().unwrap()["content"][0]["text"], "fast");
    assert!(tools[0].call(&json!({"value":"invalid"})).is_err());
}
