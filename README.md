# Prism

Prism is a Samply component ran centrally, which queries sites for criteria and number of results for these criteria in their Samply.Blaze stores. Prism is to be used by Samply.Spot or Samply.Lens for showing expected number of results next to criteria in the search tree. 

## Installation

### Standalone Installation

To run a standalone Prism, you need at least one running [Samply.Beam.Proxy](https://github.com/samply/beam/).
You can compile and run this application via Cargo.:

```bash
cargo run -- --beam-proxy-url http://localhost:8082 --beam-app-id-long app2.proxy2.broker --api-key App1Secret --bind-addr 127.0.0.1:8066 --sites proxy1 --cors-origin any --project bbmri --target-app app1
```

## Configuration

The following environment variables are mandatory for the usage of Prism.

```
--beam-proxy-url <BEAM_PROXY_URL>
    The beam proxy's base URL, e.g. https://proxy1.broker.samply.de [env: BEAM_PROXY_URL=]
--beam-app-id-long <BEAM_APP_ID_LONG>
    This application's beam AppId, e.g. prism.proxy1.broker.samply.de [env: BEAM_APP_ID_LONG=]
--api-key <API_KEY>
    This application's beam API key [env: API_KEY=]
--sites <SITES>
    Sites to initially query, separated by ';' [env: SITES=]
--cors-origin <CORS_ORIGIN>
    Where to allow cross-origin resourse sharing from [env: CORS_ORIGIN=]
--project <PROJECT>
    Project name [env: PROJECT=]
```

### Optional variables

```      
--wait-count <WAIT_COUNT>
    Wait for results count [env: WAIT_COUNT=] [default: 32]
--target-app <TARGET_APP>
    Target application name [env: TARGET_APP=] [default: focus]
--bind-addr <BIND_ADDR>
    The socket address this server will bind to [env: BIND_ADDR=] [default: 0.0.0.0:8080]
```


## Usage

Creating a sample prism query asking for criteria:

```bash
curl -v -X POST -H "Content-Type: application/json" --data '{"sites": ["proxy1"]}'  http://localhost:8066/criteria
```

If the list of the sites is empty, Prism returns the expected number of results in all the sites in its configuration.

```bash
curl -v -X POST -H "Content-Type: application/json" --data '{"sites": []}'  http://localhost:8066/criteria
```


## Roadmap

:construction: This tool is still under intensive development. Features on the roadmap are:

- [ ] Storage temperature stratifier
- [ ] GBN query 
- [ ] DKTK query


## License

This code is licensed under the Apache License 2.0. For details, please see [LICENSE](./LICENSE)
