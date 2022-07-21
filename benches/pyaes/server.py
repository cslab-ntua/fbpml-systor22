from concurrent import futures
import datetime as dt
import random
import string
import time

import pyaes

from google.protobuf.duration_pb2 import Duration
from google.protobuf.empty_pb2 import Empty
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


def generate(length):
    letters = string.ascii_lowercase + string.digits
    return "".join(random.choice(letters) for i in range(length))


KEY = b"\xa1\xf6%\x8c\x87}_\xcd\x89dHE8\xbf\xc9,"
message = generate(100)


class PyAES(fbpml_grpc.ZeroArgumentsServicer):
    def Bench(self, request: Empty, context: grpc.ServicerContext):
        response_start = time.time()
        response_duration = Duration()

        aes = pyaes.AESModeOfOperationCTR(KEY)
        _ = aes.encrypt(message)

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_ZeroArgumentsServicer_to_server(PyAES(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
