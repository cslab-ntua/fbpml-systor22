from concurrent import futures
import datetime as dt
import json
import os
import os.path
import time

from google.protobuf.duration_pb2 import Duration
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc

from minio import Minio


MINIO_ADDRESS = os.getenv("MINIO_ADDRESS")
ACCESS_KEY = "minioroot"
SECRET_KEY = "minioroot"
BUCKET_NAME = "fbpml"

TMPFS_MOUNTPOINT = "/writable_tmpfs"
DATA_NAME = ["search.json", "1.json", "2.json"]
DATA_PATH = list(map(lambda n: os.path.join(TMPFS_MOUNTPOINT, n), DATA_NAME))


class JSONSerDes(fbpml_grpc.OneArgumentServicer):
    def Bench(
        self, request: fbpml.OneArgumentRequest, context: grpc.ServicerContext
    ):
        response_start = time.time()
        response_duration = Duration()

        idx = request.arg if request.arg and 0 <= request.arg <= 2 else 0

        # Download the input JSON file from MinIO
        minio_client = Minio(
            MINIO_ADDRESS,
            access_key=ACCESS_KEY,
            secret_key=SECRET_KEY,
            secure=False,
        )
        minio_client.fget_object(BUCKET_NAME, DATA_NAME[idx], DATA_PATH[idx])

        data = open(DATA_PATH[idx]).read()
        json_data = json.loads(data)
        _ = json.dumps(json_data, indent=4)

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_OneArgumentServicer_to_server(JSONSerDes(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
