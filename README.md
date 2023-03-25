# rustio

## Optimizing network calls in Rust using Axum, Hyper, and Tokio libraries: an RCA

#### Scenario  
Nginx and Rust server are running on separate ec2 instance c6a.large in same network.

In rust server we have 2 APIs
 
1. Returns static response => Throughput **47000** requests/second
2. Make HTTP request to Nginx server -> Parse Json -> Return parsed data. => Throughput **2462** requests/second. [Issue]

For the similar benchmark in GoLang we got ~**20000** requests/second, which means there are no issues with infra/docker/client used to test rust server.

GoLang App Specs: 

http server - [Fiber](https://github.com/gofiber/fiber) with prefork enabled
JSON lib - [json-iterator](https://github.com/json-iterator/go)

Nginx request throughput ~**30000** requests/second. 
```
curl -v 172.31.50.91/
{"status": 200, "msg": "Good Luck!"}
```

The goal is to identify the cause of the performance regression in the Rust code and find ways to improve it. Some possible factors that could be causing the performance issue are:

Benchmark result
```
[ec2-user@ip-172-31-50-91 ~]$ hey -z 10s  http://172.31.50.22:80/io

Summary:
  Total:        10.0168 secs
  Slowest:      0.0692 secs
  Fastest:      0.0006 secs
  Average:      0.0203 secs
  Requests/sec: 2462.4534

  Total data:   813978 bytes
  Size/request: 33 bytes

Response time histogram:
  0.001 [1]     |
  0.007 [12766] |■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■
  0.014 [1185]  |■■■■
  0.021 [227]   |■
  0.028 [494]   |■■
  0.035 [1849]  |■■■■■■
  0.042 [3840]  |■■■■■■■■■■■■
  0.049 [3127]  |■■■■■■■■■■
  0.055 [992]   |■■■
  0.062 [174]   |■
  0.069 [11]    |
```

My attempts to improve perf.  
- Both golang and rust are running on docker container on same instance one at a time. 
- System ulimit / somaxcon has been updated to not cause any bottleneck, since static response able to perform 47K rps, it shouldn't cause limitation 
- Moved external url to lazy_static but it didn't improve performance 
```rust
lazy_static! {
    static ref EXTERNAL_URL: String = env::var("EXTERNAL_URL").unwrap();
}
```
- Tried changing tokio flavour config, workerthreads = 2, 10, 16 - it didn't improve perf. 
```rust
#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
```
- Looked into how to make sure hyper network call is being done in tokio async compatible way -> Earlier It had **247** requests/second. Was able to improve IO call by 10x via moving to stream based response processing. Reaching 2400 but still there is scope to improve. 

IO Call API - [GitHub Link](https://github.com/pratikgajjar/rustio/blob/bb893a864e3225f9448c76fa0ccaab23f9ec930c/src/main.rs#L35)
```rust
pub async fn io_call( State(state): State<AppState>) -> Json<IOCall> {
    let external_url = state.external_url.parse().unwrap();
    let client = Client::new();
    let resp = client.get(external_url).await.unwrap();
    let body = hyper::body::aggregate(resp).await.unwrap();

    Json(serde_json::from_reader(body.reader()).unwrap())
}
```

### Solution 

Thanks to [@kmdreko](https://stackoverflow.com/users/2189130/kmdreko)

Moving hyper client initialization to AppState resolved the problem. 

[Git diff](https://github.com/pratikgajjar/rustio/commit/1885fd1e56e3eae156433a8e589e61422757f4fe)

```rust
 pub async fn io_call(State(state): State<AppState>) -> Json<IOCall> {
    let external_url = state.external_url.parse().unwrap();
    let resp = state.client.get(external_url).await.unwrap();
    let body = hyper::body::aggregate(resp).await.unwrap();

    Json(serde_json::from_reader(body.reader()).unwrap())
}
```

```log
[ec2-user@ip-172-31-50-91 ~]$ hey -z 10s http://172.31.50.22:80/io


Summary:
  Total:        10.0026 secs
  Slowest:      0.0235 secs
  Fastest:      0.0002 secs
  Average:      0.0019 secs
  Requests/sec: 26876.1036

  Total data:   8871456 bytes
  Size/request: 33 bytes

Response time histogram:
  0.000 [1]     |
  0.003 [212705]        |■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■■
  0.005 [39980] |■■■■■■■■
  0.007 [10976] |■■
  0.010 [4259]  |■
  0.012 [794]   |
  0.014 [94]    |
  0.016 [17]    |
  0.019 [0]     |
  0.021 [4]     |
  0.023 [2]     |


Latency distribution:
  10% in 0.0006 secs
  25% in 0.0009 secs
  50% in 0.0013 secs
  75% in 0.0022 secs
  90% in 0.0038 secs
  95% in 0.0052 secs
  99% in 0.0083 secs

Details (average, fastest, slowest):
  DNS+dialup:   0.0000 secs, 0.0002 secs, 0.0235 secs
  DNS-lookup:   0.0000 secs, 0.0000 secs, 0.0000 secs
  req write:    0.0000 secs, 0.0000 secs, 0.0086 secs
  resp wait:    0.0018 secs, 0.0002 secs, 0.0234 secs
  resp read:    0.0001 secs, 0.0000 secs, 0.0109 secs

Status code distribution:
  [200] 268832 responses

```
