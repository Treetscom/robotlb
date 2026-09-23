# Deploying Kubernetes tutorial

For this tutorial we will be setting up HA-kubernetes cluster on hetzner using k3s.
On each server, I will be using the latest Ubuntu LTS release which is `24.04`.

We start with deploying control-plane nodes. Those nodes won't be serving any workloads,
but only responsible for managing internal Kubernetes jobs.


### Cloud servers

If you want to go full metal, order servers on the robot. But since these servers won't handle any application workloads, it's fine to deploy not-so-powerful nodes as control planes.

Let's start by ordering 5 servers. 3 servers will be used as control planes, 1 as a load balancer for Kubernetes API, and 1 as a VPN to access all these services.
You can buy 5 CAX11s which should be sufficient for small and medium-sized clusters and will cost you about 16 EUR.

### Setup users


As an additional layer of security, you can create a user called k3s on all servers which will be parts of the cluster.

```bash
useradd --comment "K3S admin user" --create-home --user-group --shell /bin/bash k3s

# Add ssh auth keys to k3s user.
mkdir /home/k3s/.ssh
cp .ssh/authorized_keys /home/k3s/.ssh/authorized_keys
chown k3s:k3s /home/k3s/.ssh/authorized_keys

# Then we add this line to the end of /etc/sudoers file
# This will allow the user to run sudo commands without password
# Required only during the k3s installation
echo "k3s ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers
```

After the installation is complete, remove the k3s from sudoers.

```bash
sed -i '/^k3s ALL.*/d' /etc/sudoers
```


### Network.

Once servers have been purchased, let's create a private network. To do so, go to a network tab and create a new network in the same zone as your servers with the subnet `10.10.0.0/16`. This subnet can be anything you want, but if you want to go with any other subnet, please make sure that it doesn't overlap with `--cluster-cidr` (used to give IPs to pods) or `--service-cidr` (used to provide IPs for services). For k3s these values can be found here: https://docs.k3s.io/cli/server#networking.

Once you have created a network, delete all the generated subnets and start making them from scratch. Let's create the first subnet for our Kubernetes servers in the cloud. It should be any IP subnet within the main one. I will go with `10.10.1.0/24` It gives me 254 possible IPs and should be generally more than enough.

### VPN

Once the network is ready, we can set up a VPN to access servers from a local machine using their private IPs.
To set up a VPN in this tutorial, I will be using WireGuard, but you can use any other VPN you are comfortable with.

Let's log in to our VPN server with its public IP and install WireGuard.

```bash
ssh root@wg-vpn
apt update
apt install -y wireguard
```

Then let's list all network interfaces and find the target interface that our private network is connected to.

```bash
$ ip addr
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN group default qlen 1000
    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00
    inet 127.0.0.1/8 scope host lo
       valid_lft forever preferred_lft forever
    inet6 ::1/128 scope host noprefixroute
       valid_lft forever preferred_lft forever
2: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel state UP group default qlen 1000
    link/ether 96:00:03:df:49:47 brd ff:ff:ff:ff:ff:ff
    inet 125.111.108.208/32 metric 100 scope global dynamic eth0
       valid_lft 82754sec preferred_lft 82754sec
    inet6 2a01:4f9:c012:dd8::1/64 scope global
       valid_lft forever preferred_lft forever
    inet6 fe80::9400:3ff:fedf:4947/64 scope link
       valid_lft forever preferred_lft forever
3: enp7s0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1450 qdisc fq_codel state UP group default qlen 1000
    link/ether 86:00:00:e9:6b:9b brd ff:ff:ff:ff:ff:ff
    inet 10.10.1.2/32 brd 10.0.0.2 scope global dynamic enp7s0
       valid_lft 82759sec preferred_lft 71959sec
    inet6 fe80::8400:ff:fee9:6b9b/64 scope link
       valid_lft forever preferred_lft forever
```

As you can see, the address `10.10.1.2/32` is assigned to the interface `enp7s0`.
Now let's create a config that will suit our needs. First, we need public and private keys for the server. Run it on the server.


```bash
wg genkey | tee privatekey | wg pubkey > publickey
```

It will generate public and private keys for the server.
Then you will need to do the same thing for any client that will be connecting to your WireGuard server.
Create a file `/etc/wireguard/wg0.conf` with the following contents (replacing keys and interface names):

```ini
[Interface]
# This is an address within Wireguard's own network.
Address    = 10.4.0.1/24
# This is our actual address in our Hetzner cloud network.
Address    = 10.10.1.2/32
# The port that the server will be listening to.
ListenPort = 5100
# PrivateKey for the server that we have generated.
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
# This enables IP forwarding before creating a new interface.
PreUp      = sysctl net.ipv4.ip_forward=1
# This command allows forwarding packets to the private network on the interface enp7s0 that we have
# looked up previously.
PostUp     = iptables -A FORWARD -i wg0 -j ACCEPT; iptables -t nat -A POSTROUTING -o enp7s0 -j MASQUERADE
# This command will remove forwarding and masquerading after the wireguard is down.
PostDown   = iptables -D FORWARD -i wg0 -j ACCEPT; iptables -t nat -D POSTROUTING -o enp7s0 -j MASQUERADE
# Removing ip_forward setting.
PostDown   = sysctl net.ipv4.ip_forward=0

# This is our client.
[Peer]
# Client's public key.
PublicKey  = BBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
# Allowed IPs for the client to claim within WireGuard's network.
AllowedIPs = 10.4.0.2/32
```

After the configuration is saved, let's enable this configuration in systemd.

```bash
systemctl enable --now wg-quick@wg0
```

Now to configure your client, create a file `/etc/wireguard/wg0.conf` on your machine with the following contents:

```ini
[Interface]
# This is an IP of your client in Wireguard's network, it should match AllowedIP on the server side.
Address    = 10.4.0.2/32
# Client's private key
PrivateKey = CCCCCCCCCCCCCCCCCCCCCCCCCCCCC=

# This is a server configuration.
[Peer]
# Server's public key.
PublicKey  = DDDDDDDDDDDDDDDDDDDDDDDDDDDDD=
# Public IP to connect to a server.
Endpoint   = 125.111.108.208:5100
# Here you set up which addresses should be routed through this peer.
# Place here your private network's subnet along with the Wireguard's network.
AllowedIPs = 10.10.0.0/16, 10.4.0.0/24
```

Once it's done, verify that this setup works, by running `wg-quick up wg0` on your machine and then try pinging any of the servers in a private subnet.

```bash
❯ ping 10.10.1.3
PING 10.10.1.3 (10.10.1.3) 56(84) bytes of data.
64 bytes from 10.10.1.3: icmp_seq=1 ttl=62 time=151 ms
64 bytes from 10.10.1.3: icmp_seq=2 ttl=62 time=89.8 ms
64 bytes from 10.10.1.3: icmp_seq=3 ttl=62 time=78.2 ms
64 bytes from 10.10.1.3: icmp_seq=4 ttl=62 time=88.0 ms

```

If it works, you are ready to proceed.

### Firewall

We don't want our nodes to be accessible from outside. To do so, we set a label on all our cloud servers like `k8s`.

Then let's create a firewall with no rules and apply it to all servers with the label we created. In my case, it will be `k8s`.

### Kube API Load balancing

To access Kubernetes API you don't want to send requests to a particular server, because if it goes down, you will have to choose the next server to connect to yourself. To fix this issue we will deploy a small load balancer that will be watching over our control-panel nodes and we will be using it as an entry point for our Kubernetes cluster.

My LoadBalancer server has IP `10.10.1.4`.

```bash
ssh root@10.10.1.4
apt update
apt install haproxy
systemctl enable --now haproxy
```

Now, once we have HA-proxy up and running we need to update its config at `/etc/haproxy/haproxy.cfg`.
Here's an example config that you can put in here:

```cfg
global
    maxconn 1000
    log 127.0.0.1 local0

defaults
    log global
    mode tcp
    retries 2
    timeout connect 5s
    timeout server 5m
    timeout check 5m

listen stats
    mode http
    bind *:80
    stats enable
    stats uri /

frontend kube-apiserver
  bind *:6443
  mode tcp

  option tcplog
  default_backend kube-apiserver

backend kube-apiserver
    mode tcp
    option tcplog
    option tcp-check
    balance roundrobin
    default-server inter 10s downinter 5s rise 2 fall 2 slowstart 60s maxconn 250 maxqueue 256 weight 100
    server kube-apiserver-1 10.10.1.1:6443 check
    server kube-apiserver-2 10.10.1.2:6443 check
    server kube-apiserver-3 10.10.1.3:6443 check
```

Then reload HA-proxy with `systemctl restart haproxy`. Now you should see your servers on the status dashboard.
Go to `http://10.10.1.4` to verfiy it.

### Deploying control panels

To do that I will be using a simple bash script. Most of the work will be done by an awesome helper [k3sup](https://github.com/alexellis/k3sup).

Important things to note here:

* We deploy with `--no-extras` argument which will disable `serviceLB` (aka `klipper`) and `traefik` ingress controller. It's important to disable klipper, because otherwise, it might conflict with `RobotLB`.
* TLS san should include load-balancer IP, otherwise your cert will be rejected.
* Make sure to use the correct flannel interface for inter-node communication.


```bash
#!/bin/bash
# SSH user to use during the installation. You can go with root if you skipped creating a user.
export K3S_USER="k3s" # or "root"
# Path to the SSH key to use during the installation.
export SSH_KEY="<path to your ssh key>"
# Load balancer IP, which will be used as the server IP for agents.
# but for now we need to put this IP in the list of allowed IPs to
# access control plane servers.
export LB_IP="10.10.1.4"
# K3s version to install.
export K3S_VERSION="v1.31.1+k3s1"
# Common arguments to control plane nodes.
# * flannel-iface: The interface for flannel to use, in this case enp7s0, which connected to the private network.
# * kube-proxy-arg=proxy-mode=ipvs: Use IPVS as the kube-proxy mode. Generally, it's more performant.
# * kube-proxy-arg=ipvs-scheduler=lc: Use the least connection scheduler for IPVS.
# * node-taint: Taint the control plane nodes as master nodes to avoid scheduling any workload on them.
export COMMON_ARGS="--flannel-iface=enp7s0 --kube-proxy-arg=ipvs-scheduler=lc --kube-proxy-arg=proxy-mode=ipvs --node-taint node-role.kubernetes.io/master=true:NoSchedule"

# Join a node to the cluster
function join_server(){
  local SERVER_IP="$1"
  local IP="$2"
  k3sup join \
    --server \
    --no-extras \
    --ip "$IP" \
    --user "$K3S_USER" \
    --k3s-extra-args "$COMMON_ARGS --node-ip=$IP" \
    --tls-san "$LB_IP,10.10.1.1,10.10.1.2,10.10.1.3" \
    --server-ip "$SERVER_IP" \
    --server-user "$K3S_USER" \
    --ssh-key "$SSH_KEY" \
    --k3s-version "$K3S_VERSION"
}

# This function is used to set up the first node,
# so it has a `--cluster` flag to initialize the cluster.
function create_cluster(){
  local SERVER_IP="$1"
  k3sup install \
      --cluster \
      --no-extras \
      --ip "$SERVER_IP" \
      --k3s-extra-args "$COMMON_ARGS --node-ip=$SERVER_IP" \  # note that node-ip. It will be used by Robotlb to create a cloud load-balancer.
      --tls-san "$LB_IP,10.10.1.1,10.10.1.2,10.10.1.3" \
      --user "$K3S_USER" \
      --ssh-key "$SSH_KEY" \
      --k3s-version "$K3S_VERSION"
}

# We have 3 control plane servers
# 10.10.1.1
# 10.10.1.2
# 10.10.1.3
# Here we deploy the first control plane server
create_cluster "10.10.1.1"
# Connect the second server to the first
join_server "10.10.1.1" "10.10.1.2"
# Connect the third server to the second
join_server "10.10.1.2" "10.10.1.3"
```

After running this script you should see that control planes became healthy on our load-balancer at `http://10.10.1.4`. Also, in your generated kubeconfig file change the address of the server to the load-balancer's address. So it will become:


```yaml
...
    server: https://10.10.1.4:6443
...
```

Now try running kubectl with this kubeconfig.

```bash
$ export KUBECONFIG=kubeconfig
$ kubectl cluster-info

Kubernetes control plane is running at https://10.10.1.4:6443
CoreDNS is running at https://10.10.1.4:6443/api/v1/namespaces/kube-system/services/kube-dns:dns/proxy
Metrics-server is running at https://10.10.1.4:6443/api/v1/namespaces/kube-system/services/https:metrics-server:https/proxy

To further debug and diagnose cluster problems, use 'kubectl cluster-info dump'.
```

You should see that all IPs are pointing to a load-balancer.

# Robot side

Now let's create agent nodes on the Hetzner robot. I will buy 2 servers in the same region as our cloud servers.

After they are ready, create a `vSwitch` and connect robot servers to the switch. But before setting up all the virtual interfaces,
create a `vSwitch` type subnet in our cloud network.


### vSwitch backbone


To do so, go to a cloud `network console` > `subnets` > `add subnet` and select a subnet range that you think will be sufficient for all your robot servers. Keep in mind that you can add only one `vSwitch` to a cloud network. I would go with `10.10.192.0/18`, because I want to also host dedicated servers for other purposes, like databases and other things.

After this is ready, you need to expose routes to the `vSwitch` by clicking on three dots after the `vSwitch` subnet and choosing the appropriate option in the dropdown.

### Network interfaces

After you have chosen the subnet, Hetzner will give you the gateway address that will be used to communicate with cloud servers. Generally, it's the first IP in a chosen subnet. In my scenario, it will be `10.10.192.1`.

Let's log in to our robot servers by their public IPs and set up private networking interfaces. I won't be using the `ip` command as suggested in a hint by Hetzner, instead, I will be using `netplan` to make my interface setup persistent.

```bash
ssh treets-prod-kube-agent-1
vim /etc/netplan/01-netcfg.yaml
```

Here create a new VLAN object inside of the network object.

```yaml
network:
    ... # here goes some default network configuration. Don't touch it.

    vlans:
        enp6s0.4000:  # You can name it anyway, but please make the same name for all your servers in this VLAN.
          id: 4000    # This is a VLAN id that was chosen when creating vSwitch.
          link: enp6s0  # Name of the physical interface. Typically you will have a similar name.
                        # Check all available interfaces by running `ip addr`
          mtu: 1400  # Don't forget to set it, otherwise some packets might mess up.
          addresses:
            - 10.10.255.1/18  # Here goes the address of this node. Please note that you should use /18, because there's only one subnet
                              # for all dedicated servers. Therefore for all the IPs in the addresses section, the subnet mask should be /18.
          routes: # That section means that to reach any addresses in our cloud network we should go to the gateway.
            - to: "10.10.0.0/16"
              via: "10.10.192.1"
```

After this is ready, apply the plan and try pinging the load balancer.

```bash
netplan generate
netplan apply
ping 10.10.1.4
```

Then do the same thing for the second agent server replacing its address with the one you want. I will go with `10.10.255.2/18`.

By the way, if you want to get more info about how the connection between the cloud network and vSwitch works, you can follow this tutorial:
https://docs.hetzner.com/cloud/networks/connect-dedi-vswitch/

### Firewall

Once robot servers are accessible from the private network, we can hide them with the firewall. I would recommend
you to only hide ports that we don't want to expose, because of possible DNS issues.

### Deploying agents

Finally, we got to a part where we started deploying agent nodes. To do so, we will use another bash script.

I will create a user `k3s` as I have done for control planes, but you can use `root` if you don't want it.

```bash
#!/bin/bash

# Ssh user to use during the installation.
export K3S_USER="k3s"
# SSH key to use during the installation.
export SSH_KEY="<your ssh key>"
# Here's a main server, we will use it to get TOKEN for joining
# to the cluster.
export MAIN_SERVER_IP="10.10.1.1"
# Load balancer IP, which will be used as the server IP
# because it load balances the requests to the control plane servers.
# If any of the control plane nodes will go down, agents will reconnect to another automatically.
export LB_IP="10.10.1.4"
# Some extra arguments for the agents:
# * iflannel-iface: The interface for flannel to use, in this case enp6s0.4000, which is connected to the vSwitch.
# * kube-proxy-arg=proxy-mode=ipvs: Use IPVS as the kube-proxy mode.
# * kube-proxy-arg=ipvs-scheduler=lc: Use the least connection scheduler for IPVS.
export K3S_EXTRA_ARGS="--flannel-iface=enp6s0.4000 --kube-proxy-arg=ipvs-scheduler=lc --kube-proxy-arg=proxy-mode=ipvs"
# K3S version to install.
export K3S_VERSION="v1.31.1+k3s1"

# Join a node to the cluster
function join_agent(){
  local SERVER_IP="$1"
  local TOKEN="$2"
  local IP="$3"
  # again, node-ip argument is important for robotlb to work.
  k3sup join \
    --ip "$IP" \
    --user "$K3S_USER" \
    --k3s-extra-args "$K3S_EXTRA_ARGS --node-ip=$IP" \
    --server-ip "$SERVER_IP" \
    --server-user "$K3S_USER" \
    --ssh-key "$SSH_KEY" \
    --node-token "$TOKEN" \
    --k3s-version "$K3S_VERSION"
}

# Here we get our token from the main node. It will be used to join the agents.
TOKEN="$(k3sup node-token --ip $MAIN_SERVER_IP --ssh-key "$SSH_KEY" --user "$K3S_USER")"

join_agent "$LB_IP" "$TOKEN" "10.10.255.1"
join_agent "$LB_IP" "$TOKEN" "10.10.255.2"
```

### Deploying RobotLB

Once the cluster is ready, we can deploy our RobotLB. Before that, create an hcloud token as shown here: https://docs.hetzner.com/cloud/api/getting-started/generating-api-token/.

```bash
helm install robotlb  \
    oci://ghcr.io/treetscom/charts/robotlb \
    --set envs.ROBOTLB_HCLOUD_TOKEN="<hcloud token>" \
    --namespace robotlb \
    --create-namespace \
    --wait
```

After service LB is deployed, we can deploy the NGINX ingress controller to verify installation. I will set it up as a `DaemonSet` so it will be deployed on all agent nodes.

Here are the values for helm, that I'm going to use for nginx.

```yaml
# nginx-values.yaml
controller:
  kind: DaemonSet
  admissionWebhooks:
    enabled: false
  ingressClassResource:
    default: true
  service:
    type: LoadBalancer
    annotations:
      robotlb/lb-network: "<name of your cloud network>"
      robotlb/balancer-type: "lb11"
      robotlb/lb-algorithm: "least-connections"
    externalTrafficPolicy: "Local"
```

For the full list of parameters, check out `README.md`.

```bash
helm repo add ingress-nginx https://kubernetes.github.io/ingress-nginx
helm install ingress-nginx \
    ingress-nginx/ingress-nginx \
    --namespace ingress-nginx \
    --create-namespace \
    --wait \
    --values nginx-values.yaml
```

Once the nginx is deployed, verify that the load-balancer is created on Hetzner cloud console. And check that external-ip for the service of type LoadBalancer is an actual public IP of a cloud loadbalancer.
