# Mesos demo game

This is a repository powering https://hub.docker.com/r/zaynetro/mesos-demo-game/ docker image.

Workflow:

```
docker build -t zaynetro/mesos-demo-game .
docker run -t -p 3000:3000 zaynetro/mesos-demo-game
docker push zaynetro/mesos-demo-game
```
