from concurrent import futures
import datetime as dt
import time

from google.protobuf.duration_pb2 import Duration
from google.protobuf.empty_pb2 import Empty
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


class Greeter(fbpml_grpc.ZeroArgumentsServicer):
    def Bench(self, request: Empty, context: grpc.ServicerContext):
        response_start = time.time()
        response_duration = Duration()
        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_ZeroArgumentsServicer_to_server(Greeter(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
