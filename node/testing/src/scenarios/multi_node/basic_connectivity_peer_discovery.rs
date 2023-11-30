use std::time::Duration;

use node::event_source::Event;
use node::p2p::connection::outgoing::P2pConnectionOutgoingInitOpts;
use node::p2p::P2pEvent;
use tokio::task::JoinSet;

use crate::scenarios::cluster_runner::ClusterRunner;
use crate::{node::RustNodeTestingConfig, scenario::ScenarioStep};

use crate::ocaml::{Node, self};

/// Global test with OCaml nodes.
/// Run an OCaml node as a seed node. Run three normal OCaml nodes connecting only to the seed node.
/// Wait 3 minutes for the OCaml nodes to start.
/// Run the Openmina node (application under test).
/// Wait for the Openmina node to complete peer discovery and connect to all OCaml nodes.
/// (This step ensures that the Openmina node can discover OCaml nodes).
/// Run another OCaml node that connects only to the seed node.
/// So this additional OCaml node only knows the address of the seed node.
/// Wait for the additional OCaml node to initiate a connection to the Openmina node.
/// (This step ensures that the OCaml node can discover the Openmina node).
/// Fail the test on the timeout.
#[derive(documented::Documented, Default, Clone, Copy)]
pub struct MultiNodeBasicConnectivityPeerDiscovery;

impl MultiNodeBasicConnectivityPeerDiscovery {
    pub async fn run(self, mut runner: ClusterRunner<'_>) {
        const STEPS: usize = 4_000;
        const STEP_DELAY: Duration = Duration::from_millis(200);
        const TOTAL_OCAML_NODES: u16 = 4;
        const PAUSE_UNTIL_OCAML_NODES_READY: Duration = Duration::from_secs(30 * 60);

        let temp_dir = temp_dir::TempDir::new().unwrap();

        let mut seed_a = Node::spawn::<_, &str>(8302, 3085, 8301, &temp_dir.path().join("seed"), []).expect("seed ocaml node");
        eprintln!("launching OCaml seed node: {}", seed_a.local_addr());

        tokio::time::sleep(Duration::from_secs(60)).await;

        let nodes = (1..TOTAL_OCAML_NODES)
            .map(|i| {
                let n = Node::spawn(
                    8302 + i * 10,
                    3085 + i * 10,
                    8301 + i * 10,
                    &temp_dir.path().join(i.to_string()),
                    [seed_a.local_addr().to_string()],
                ).expect("ocaml node");
                eprintln!("launching OCaml node {}", n.local_addr());
                n
            })
            .collect::<Vec<_>>();


        // wait for ocaml nodes to be ready
        let mut join_set = JoinSet::new();
        for n in &nodes {
            let w = ocaml::wait_for_port_ready(n.port, PAUSE_UNTIL_OCAML_NODES_READY);
            join_set.spawn(w);
        }
        while let Some(res) = join_set.join_next().await {
            assert!(res.unwrap().unwrap(), "OCaml node should be ready");
        }
        eprintln!("OCaml nodes should be ready now");

        let config = RustNodeTestingConfig::berkeley_default()
            .ask_initial_peers_interval(Duration::from_secs(3600))
            .max_peers(100)
            .libp2p_port(10000)
            .initial_peers(
                nodes
                    .iter()
                    .chain(std::iter::once(&seed_a))
                    .map(|n| P2pConnectionOutgoingInitOpts::try_from(&n.local_addr()).unwrap())
                    .collect(),
            );
        let node_id = runner.add_rust_node(config);
        eprintln!("launching Openmina node {node_id}");

        let mut additional_ocaml_node = None::<Node>;

        let mut timeout = STEPS;
        loop {
            if timeout == 0 {
                break;
            }
            timeout -= 1;

            tokio::time::sleep(STEP_DELAY).await;

            let steps = runner
                .pending_events()
                .map(|(node_id, _, events)| {
                    events.map(move |(_, event)| {
                        match event {
                            Event::P2p(P2pEvent::Discovery(event)) => {
                                eprintln!("event: {event}");
                            }
                            _ => {}
                        }
                        ScenarioStep::Event {
                            node_id,
                            event: event.to_string(),
                        }
                    })
                })
                .flatten()
                .collect::<Vec<_>>();

            for step in steps {
                runner.exec_step(step).await.unwrap();
            }

            runner
                .exec_step(ScenarioStep::AdvanceNodeTime {
                    node_id,
                    by_nanos: STEP_DELAY.as_nanos() as _,
                })
                .await
                .unwrap();

            runner
                .exec_step(ScenarioStep::CheckTimeouts { node_id })
                .await
                .unwrap();

            let this = runner.node(node_id).unwrap();
            // finish discovering
            if this.state().p2p.kademlia.saturated.is_some() {
                // the node must find all already running OCaml nodes
                // assert_eq!(this.state().p2p.peers.len(), TOTAL_OCAML_NODES as usize);
                additional_ocaml_node.get_or_insert_with(|| {
                    let n = Node::spawn(9000, 4000, 9001, &temp_dir.path().join("add"), [seed_a.local_addr().to_string()]).expect("additional ocaml node");
                    eprintln!("the Openmina node finished peer discovery",);
                    eprintln!(
                        "connected peers: {:?}",
                        this.state().p2p.peers.keys().collect::<Vec<_>>()
                    );
                    eprintln!("launching additional OCaml node {}", n.local_addr());
                    n
                });
            }

            if let Some(additional_ocaml_node) = &additional_ocaml_node {
                let peer_id = additional_ocaml_node.peer_id();

                if runner
                    .node(node_id)
                    .unwrap()
                    .state()
                    .p2p
                    .peers
                    .iter()
                    .filter(|(id, n)| n.is_libp2p && *id == &peer_id)
                    .filter_map(|(_, n)| n.status.as_ready())
                    .find(|n| n.is_incoming)
                    .is_some()
                {
                    eprintln!("the additional OCaml node connected to Openmina node");
                    eprintln!("success");

                    break;
                }
            }
        }

        for mut node in nodes {
            node.kill();
        }
        seed_a.kill();
        additional_ocaml_node.as_mut().map(Node::kill);

        if timeout == 0 {
            panic!("timeout");
        }
    }
}
