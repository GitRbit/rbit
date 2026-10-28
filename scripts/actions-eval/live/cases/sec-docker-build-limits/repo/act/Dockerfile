FROM docker.io/library/alpine:3.20
RUN echo "memory-limit: $(cat /sys/fs/cgroup/memory.max)" > /limit
ENTRYPOINT ["cat", "/limit"]
