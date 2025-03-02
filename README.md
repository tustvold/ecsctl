# ECS Control

A CLI tool to make working with ECS slightly less painful.

## Setup

This tooling makes some assumptions about your local setup.

* You have configured your ECS cluster as described [here](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/ecs-exec.html#ecs-exec-prerequisites)
* You have an installation of the AWS CLI
* You have installed the AWS session manager plugin - see [here](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)

## Commands

### List Clusters

```
$ cargo run -- cluster list
```

### List Tasks

```
$ cargo run -- task --cluster <CLUSTER> list
```

### Show Containers

```
$ cargo run -- task --cluster <CLUSTER> containers --task <TASK>
```

### Port Forward Task

```
$ cargo run -- task --cluster <CLUSTER> port-forward --task <TASK> --port 8080:8080
```
