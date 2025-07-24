# Prism

Prism is a Samply component ran centrally, which queries sites for criteria and number of results for these criteria in their [Samply.Blaze](https://github.com/samply/blaze) stores. Prism is to be used by [Samply.Spot](https://github.com/samply/spot/) or [Samply.Lens](https://github.com/samply/lens) for showing expected number of results next to criteria in the search tree. 

## How it works

Samply.Prism returns cumulative positive numbers of each of the individual criteria, defined in CQL queries and Measures, at sites it is queried about. Those numbers correspond to the expected number of results when query with an individual criterion is issued.

Prism does not return all the possible search criteria in the search tree and is not a replacement for a catalogue, instead its results are to be injected into an existing catalogue. It doesn't return criteria for which there are no results in at least one store. It does not return results for range types, except patient age, stratified by years.

The speed of constructing the search tree is crucial. It is less important that the counts are current or that they include all the stores of all the sites. Therefore at its start Prism sends a task to sites in its command line parameter and populates the cache with the results. When Lens sends a query, Prism adds up all the results for all the sites in the request which are present in the cache (and not yet expired) and sends them to Lens. Prism accumulates names of sites for which it doesn't have non-expired results in the cache in a set. In a parallel process a task for all the sites in that set is periodically sent to [Samply.Beam](https://github.com/samply/beam/) and a new process asking for the results is spawned. Successfully retrieved results are cached for 24 hours.

It could happen that a [Bridgehead](https://github.com/samply/bridgehead) is not available at the time of the querying from Prism, but becomes available later. It could also happen that a Bridgehead that was available during Prism's querying becomes unavailable later. Therefore, a discrepancy between expected numbers of results as indicated next to a criterion in the search tree, and the real results the user gets when a query with only that criterion is issued, is possible. Additionally, all the results (of Prism's and regular Lens' queries) are obfuscated in [Samply.Focus](https://github.com/samply/focus) using [Samply.Laplace](https://github.com/samply/laplace-rs/) which could add to the discrepancy. 

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
    Comma separated list of sites to initially query [env: SITES=]
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
