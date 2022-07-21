PY_GRPC_IMG := ckatsak/pandas-grpc
PY_GRPC_TAG := 1.3.5-1.43-python3.10.1-alpine3.15

TARGET := functionbench_pmem_local_pb2.py functionbench_pmem_local_pb2_grpc.py


.PHONY: all
all: functionbench_pmem_local_pb2_grpc.py

functionbench_pmem_local_pb2_grpc.py: functionbench_pmem_local_pb2.py

functionbench_pmem_local_pb2.py: functionbench_pmem_local.proto
	docker run --rm -it \
		--user "$(shell id -u):$(shell id -g)" \
		--volume "$(CURDIR)":'/src' \
		$(PY_GRPC_IMG):$(PY_GRPC_TAG) \
		sh -c "python -m grpc_tools.protoc \
			-I/src \
			--python_out=/src \
			--grpc_python_out=/src \
			/src/$<"

.PHONY: clean
clean:
	$(RM) -v $(TARGET)
