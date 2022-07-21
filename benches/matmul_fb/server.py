from concurrent import futures
import datetime as dt
import time

import numpy as np

from google.protobuf.duration_pb2 import Duration
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


class FunctionBenchMatMul(fbpml_grpc.TwoArgumentsServicer):
    def Bench(
        self,
        request: fbpml.TwoArgumentsRequest,
        context: grpc.ServicerContext,
    ):
        response_start = time.time()
        response_duration = Duration()

        N = request.arg1 if request.arg1 and request.arg1 > 0 else 512
        M = request.arg2 if request.arg2 and request.arg2 > 0 else 512

        A = np.random.rand(N, M)  # NxM
        B = np.random.rand(M, M)  # MxN
        _ = np.matmul(A, B)  # NxN

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_TwoArgumentsServicer_to_server(
        FunctionBenchMatMul(), server
    )
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
