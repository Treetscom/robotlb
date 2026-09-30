# Hetzner LoadBalancer for bare-metal robot clusters

This project is useful when you've deployed a bare-metal Kubernetes cluster on Hetzner Robot and want to use Hetzner's cloud load balancer.

This small operator integrates them together, allowing you to use the `LoadBalancer` service type.

You can follow the [TUTORIAL.md](./tutorial.md) to see how to set up a cluster using RobotLB from scratch.

## Prerequisites

Before using this operator, make sure:

1. You have a cluster deployed on [Hetzner robot](https://robot.hetzner.com/) (at least agent nodes);
2. You've created a [vSwitch](https://docs.hetzner.com/robot/dedicated-server/network/vswitch/) for these servers;
3. You've assigned IPs to your dedicated servers within the vSwitch network.
4. You have a cloud network with subnet that points to the vSwitch ([Tutorial](https://docs.hetzner.com/cloud/networks/connect-dedi-vswitch/));
5. You’ve specified node IPs using the `--node-ip` argument with the private IP.

If you meet all the requirements, you can deploy `robotlb`.

## Deploying

The recommended way to deploy this operator is using the Helm chart.

```bash
helm show values oci://ghcr.io/treetscom/charts/robotlb > values.yaml
# Edit values.yaml to suit your needs
# Set `envs.ROBOTLB_HCLOUD_TOKEN`.
helm install robotlb oci://ghcr.io/treetscom/charts/robotlb -f values.yaml
```

After the chart is installed, you should be able to create `LoadBalancer` services.

## How it works

The operator listens to the Kubernetes API for services of type `LoadBalancer` and creates Hetzner load balancers that point to nodes based on `node-ip`.

A balancer is updated when its service changes, when a node is added, removed, relabelled, cordoned or changes readiness or addresses, and, for services with `externalTrafficPolicy: Local` while `ROBOTLB_DYNAMIC_NODE_SELECTOR` is on, when the nodes their endpoints serve traffic from change; a deleted endpoint slice of such a service rechecks every balancer. Apart from that robotlb checks each balancer every `ROBOTLB_RESYNC_INTERVAL` seconds, 300 by default, which bounds how long a change made to the balancer in Hetzner survives. Each check costs one or two Hetzner API requests per service. When Hetzner refuses a target for a reason other than the rate limit, the service is checked again within 30 seconds instead, so a node it refuses for good, such as one outside the vSwitch subnet, keeps its service on that 30-second cycle.

Target nodes are selected according to the service's `externalTrafficPolicy`:

- `Cluster`, the Kubernetes default: every node of the cluster becomes a target, since kube-proxy forwards the traffic to a node that hosts a pod. Cordoned and not-ready nodes are left out, as they would only take up target slots.
- `Local`: only the nodes kube-proxy serves the traffic from, read from the service's `EndpointSlice` resources whether or not the service has a selector. These are the nodes with a ready endpoint, or with a terminating endpoint that still serves while the node has no ready one. Endpoint conditions decide, not whether a pod runs on the node: a node whose pods are not ready is left out, and a cordoned or not-ready node stays a target while it has such an endpoint.

Nodes labelled `node.kubernetes.io/exclude-from-external-load-balancers`, which kubeadm puts on control-plane nodes, stay out of every balancer under either policy.

Setting `ROBOTLB_DYNAMIC_NODE_SELECTOR` to `false` replaces both with the node selector from the `robotlb/node-selector` annotation.

A balancer type caps how many targets it holds: `lb11`, the default type, holds 25. When more nodes are selected than the type holds, the extra ones are dropped in a stable order and a warning names the limit. Pick a bigger type through `ROBOTLB_DEFAULT_LB_TYPE` or the `robotlb/balancer-type` annotation to use the whole cluster.

robotlb labels every balancer it creates with `robotlb/service-uid` set to the UID of the service, finds the balancer of a service by that label, and changes or deletes only a balancer labelled for that service. A balancer is never deleted by its name. Without the `robotlb/balancer` annotation the balancer is named `<service>.<namespace>`, or `<cluster>.<service>.<namespace>` when `ROBOTLB_CLUSTER_NAME` is set, so that clusters sharing a Hetzner project do not pick the same name. A name longer than the 128 characters Hetzner allows is cut and ends in `-` and a 16-digit hex hash of the whole name. The cluster name is not added to a name from the annotation. Balancers that already carry the `robotlb/service-uid` label are found by it, so setting a cluster name later does not rename them. The name is only used to create the balancer: changing the annotation later does not rename it. When the name is taken by a balancer labelled for another service, or by an unlabelled balancer that does not target the nodes of the service, robotlb leaves that balancer alone and reports it in a warning event on the service. Two services therefore cannot share a balancer through the same `robotlb/balancer` annotation: the second one gets the warning. A service recreated with a new UID, for example restored from a backup, is another service to robotlb, because its old balancer is labelled with the old UID. When that balancer has the name the service asks for, the warning names the old UID. When it has the name of an earlier release, robotlb creates a new balancer with a new IP instead, without a warning, and the old one stays in the project and keeps being billed; `hcloud load-balancer list --selector robotlb/service-uid=<old UID>` finds it. After checking that no service with the old UID is left, hand the balancer over with `hcloud load-balancer add-label --overwrite '<balancer>' robotlb/service-uid=<new UID>`, or delete it. An unlabelled balancer whose targets are all IPs, at least one of them a node of the service, is adopted: robotlb adds its label, keeps the name, and from then on manages and deletes it like its own. This is how balancers of earlier releases are taken over, and it applies to any balancer created later under that name that matches.

Every port of the service needs an allocated `nodePort`. A Hetzner load balancer forwards traffic to the IP of a node, so a port is reachable only through its `nodePort`: ports without one are skipped, and `allocateLoadBalancerNodePorts: false` is not supported. When no port of a service can be exposed, no balancer is created for it, an existing balancer and the service's external IP are kept as they are, and a warning event on the service reports the problem.

> Earlier releases treated every service as if it had the `Local` policy. Services that leave `externalTrafficPolicy` unset therefore get the full node list on upgrade, which changes the targets of their existing balancers.

> Earlier releases created balancers without a label and named them after the service without its namespace. On the first reconcile after the upgrade, a service with no labelled balancer adopts the balancer under its old name, or under the name from its `robotlb/balancer` annotation, if every target of that balancer is an IP and at least one of them is a node of the service: robotlb adds its label and keeps the name. Otherwise the balancer is left alone: under the old name robotlb creates a new balancer with a new IP under `<service>.<namespace>`, under an annotated name the service gets a warning event instead. Services with the same name in different namespaces used to share one balancer: one of them keeps it, the others get a new balancer and a new IP. When they reconcile at the same moment, the others may configure the shared balancer and report its address once more before they get their own. A balancer without targets, for example because Hetzner refused every node, does not look like the service's. Under the old name it is replaced by a new balancer with a new IP and stays in the project, unlabelled and billed; under an annotated name the service gets a warning event. To hand an unlabelled balancer to a service yourself, label it: `hcloud load-balancer add-label '<balancer>' robotlb/service-uid=<service UID>`. While a service has no target nodes, for example a `Local` service without ready pods, robotlb cannot tell whether an unlabelled balancer is its own, so it waits and reports that in a warning event.
>
> A balancer is deleted only when it carries the label, so the balancer of a service deleted or changed from `LoadBalancer` before its first successful reconcile on this release stays in the project: a reconcile that stops early, for example because the service has no port to expose or no target nodes, adopts nothing. This includes services that earlier releases kept a balancer for after their type changed from `LoadBalancer` or they moved to another load balancer class: they still carry the `robotlb/finalizer` finalizer, robotlb removes it on the first start, and their balancers stay. List the balancers without the label, check which of them are still used, and delete the rest:
>
> ```bash
> hcloud load-balancer list --selector '!robotlb/service-uid'
> ```
>
> The services that lose their finalizer this way can be listed before the upgrade:
>
> ```bash
> kubectl get services --all-namespaces --output json | jq --raw-output '.items[] | select(((.metadata.finalizers // []) | index("robotlb/finalizer")) and (.spec.type != "LoadBalancer" or (.spec.loadBalancerClass // "robotlb") != "robotlb")) | "\(.metadata.namespace)/\(.metadata.name)"'
> ```

## Configuration

This project has two places for configuration: environment variables and service annotations.

### Envs

Environment variables are mainly used to override default arguments and provide sensitive information.

Here’s a complete list of parameters for the operator's binary:

```text
Usage: robotlb [OPTIONS] --hcloud-token <HCLOUD_TOKEN>

Options:
  -t, --hcloud-token <HCLOUD_TOKEN>
          `HCloud` API token [env: ROBOTLB_HCLOUD_TOKEN=]
      --cluster-name <CLUSTER_NAME>
          Name of the cluster, put in front of default balancer names so that clusters sharing a Hetzner project do not pick the same ones. A DNS label: lowercase letters, digits and `-`, at most 63 characters [env: ROBOTLB_CLUSTER_NAME=]
      --default-network <DEFAULT_NETWORK>
          Default network to use for load balancers. If not set, then only network from the service annotation will be used [env: ROBOTLB_DEFAULT_NETWORK=]
      --dynamic-node-selector
          If enabled, the operator will try to find target nodes based on the service's traffic policy, and under `Local` on the nodes serving the service's endpoints. If disabled, the operator will try to find target nodes based on the node selector [env: ROBOTLB_DYNAMIC_NODE_SELECTOR=]
      --default-lb-retries <DEFAULT_LB_RETRIES>
          Default load balancer healthcheck retries cound [env: ROBOTLB_DEFAULT_LB_RETRIES=] [default: 3]
      --default-lb-timeout <DEFAULT_LB_TIMEOUT>
          Default load balancer healthcheck timeout [env: ROBOTLB_DEFAULT_LB_TIMEOUT=] [default: 10]
      --default-lb-interval <DEFAULT_LB_INTERVAL>
          Default load balancer healhcheck interval [env: ROBOTLB_DEFAULT_LB_INTERVAL=] [default: 15]
      --default-lb-location <DEFAULT_LB_LOCATION>
          Default location of a load balancer. https://docs.hetzner.com/cloud/general/locations/ [env: ROBOTLB_DEFAULT_LB_LOCATION=] [default: hel1]
      --default-balancer-type <DEFAULT_BALANCER_TYPE>
          Type of a load balancer. It differs in price, number of connections, target servers, etc. The default value is the smallest balancer. https://docs.hetzner.com/cloud/load-balancers/overview#pricing [env: ROBOTLB_DEFAULT_LB_TYPE=] [default: lb11]
      --default-lb-algorithm <DEFAULT_LB_ALGORITHM>
          Default load balancer algorithm. Possible values: * `least-connections` * `round-robin` https://docs.hetzner.com/cloud/load-balancers/overview#load-balancers [env: ROBOTLB_DEFAULT_LB_ALGORITHM=] [default: least-connections]
      --default-lb-proxy-mode-enabled
          Default load balancer proxy mode. If enabled, the load balancer will act as a proxy for the target servers. The default value is `false`. https://docs.hetzner.com/cloud/load-balancers/faq/#what-does-proxy-protocol-mean-and-should-i-enable-it [env: ROBOTLB_DEFAULT_LB_PROXY_MODE_ENABLED=]
      --ipv6-ingress
          Whether to enable IPv6 ingress for the load balancer. If enabled, the load balancer's IPv6 will be attached to the service as an external IP along with IPv4 [env: ROBOTLB_IPV6_INGRESS=]
      --resync-interval <RESYNC_INTERVAL>
          Seconds between reconciliations of a service that nothing changed. Node changes, and endpoint changes of Local services, trigger a reconciliation on their own; this interval bounds how long a change made to a balancer outside robotlb survives. A service whose balancer refused a target is retried within 30 seconds [env: ROBOTLB_RESYNC_INTERVAL=] [default: 300]
      --log-level <LOG_LEVEL>
          [env: ROBOTLB_LOG_LEVEL=] [default: INFO]
  -h, --help
          Print help
```

### Service annotations

```yaml
apiVersion: v1
kind: Service
metadata:
  name: target
  annotations:
    # Name of the balancer to create on Hetzner. Defaults to <service>.<namespace>.
    robotlb/balancer: "custom name"
    # Hetzner cloud network. If this annotation is missing, the operator will try to
    # assign external IPs to the load balancer if available. Otherwise, the update won't happen.
    robotlb/lb-network: "my-net"
    # Requests specific IP address for the load balancer in the private network. If not specified,
    # a random one is given. This parameter does nothing in case if network is not specified.
    robotlb/lb-private-ip: "10.10.10.10"
    # Node selector for the loadbalancer. This is only required if ROBOTLB_DYNAMIC_NODE_SELECTOR
    # is set to false. If not specified then, all nodes will be selected as LB targets by default.
    # This property helps you filter out nodes.
    # Filters are separated by commas and should have one of the following formats:
    # * key=value  -- checks that the node has a label `key` with value `value`;
    # * key!=value -- verifies that key either doesn't exist or isn't equal to `value`;
    # * !key       -- verifies that the node doesn't have a label `key`;
    # * key        -- verifies that the node has a label `key`.
    robotlb/node-selector: "node-role.kubernetes.io/control-plane!=true,beta.kubernetes.io/arch=amd64"
    ### Load balancer healthcheck options. ###
    # How often to run health probes.
    robotlb/lb-check-interval: "5"
    # Timeout for a single probe.
    robotlb/lb-timeout: "3"
    # How many failed probes before marking the node as unhealthy.
    robotlb/lb-retries: "3"

    ### Load balancer options ###
    # Whether to use proxy mode for this target.
    # https://docs.hetzner.com/cloud/load-balancers/faq/#what-does-proxy-protocol-mean-and-should-i-enable-it
    robotlb/lb-proxy-mode: "false"
    # Location of the load balancer. This expects the code of one of Hetzner's available locations.
    robotlb/lb-location: "hel1"
    # Balancing algorithm. Can be either
    # * least-connection
    # * round-robin
    robotlb/lb-algorithm: "least-connection"
    # Type of balancer.
    robotlb/balancer-type: "lb11"
spec:
  type: LoadBalancer
  # The selector fills the service's endpoints; with
  # externalTrafficPolicy: Local and the dynamic node selector
  # enabled, the nodes serving them become the targets.
  selector:
    app: target
  ports:
    # Currently only TCP protocol is supported. UDP will be ignored.
    - protocol: TCP
      port: 80  # This will become the listening port on the LB
      targetPort: 80
```

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Treetscom/robotlb&type=Date)](https://star-history.com/#Treetscom/robotlb&Date)
