from concurrent import futures
import datetime as dt
import time

from chameleon import PageTemplate
import six

from google.protobuf.duration_pb2 import Duration
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


BIGTABLE_ZPT = (
    """\
<table xmlns="http://www.w3.org/1999/xhtml"
xmlns:tal="http://xml.zope.org/namespaces/tal">
<tr tal:repeat="row python: options['table']">
<td tal:repeat="c python: row.values()">
<span tal:define="d python: c + 1"
tal:attributes="class python: 'column-' + %s(d)"
tal:content="python: d" />
</td>
</tr>
</table>"""
    % six.text_type.__name__
)


class Chameleon(fbpml_grpc.TwoArgumentsServicer):
    def Bench(
        self,
        request: fbpml.TwoArgumentsRequest,
        context: grpc.ServicerContext,
    ):
        response_start = time.time()
        response_duration = Duration()

        num_of_rows = request.arg1  # 10
        num_of_cols = request.arg2  # 15

        data = {}
        for i in range(num_of_cols):
            data[str(i)] = i

        table = [data for _ in range(num_of_rows)]
        options = {"table": table}

        tmpl = PageTemplate(BIGTABLE_ZPT)
        data = tmpl.render(options=options)

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_TwoArgumentsServicer_to_server(Chameleon(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
