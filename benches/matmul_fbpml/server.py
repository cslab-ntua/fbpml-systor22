from concurrent import futures
import datetime as dt
import time

import numpy as np

from google.protobuf.duration_pb2 import Duration
from google.protobuf.empty_pb2 import Empty
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


N = M = 512
A = np.random.rand(N, M)  # NxM
B = np.random.rand(M, M)  # MxN


class FbpmlMatMul(fbpml_grpc.ZeroArgumentsServicer):
    def Bench(self, request: Empty, context: grpc.ServicerContext):
        response_start = time.time()
        response_duration = Duration()

        _ = np.matmul(A, B)  # NxN

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_ZeroArgumentsServicer_to_server(FbpmlMatMul(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
