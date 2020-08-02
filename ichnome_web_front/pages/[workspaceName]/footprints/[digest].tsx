import { NextPage } from "next";
import Head from "next/head";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";
import { defaultInstance } from "@/api/apiClient";
import { applicationName } from "@/config";
import { GetFootprintResponse } from "@/api/types";
import FootprintView from "@/components/Footprint";
import HistoryGroup from "@/components/HistoryGroup";
import StatGroup from "@/components/StatGroup";
import GlobalNav from "@/components/GlobalNav";

type Query = {
  workspaceName: string;
  digest: string;
};

type Response = GetFootprintResponse;

type Props = { response?: Response; err?: string };

const ResponseView: React.FC<{ response: Response }> = ({ response: { footprint, stats, histories } }) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { workspaceName } = (rawQuery as unknown) as Query;
  return (
    <>
      <h2>Footprint</h2>
      <FootprintView workspaceName={workspaceName} footprint={footprint} />
      {stats != null ? (
        <>
          <h2>Stats</h2>
          <StatGroup workspaceName={workspaceName} stats={stats} />
        </>
      ) : undefined}
      {histories != null ? (
        <>
          <h2>Histories</h2>
          <HistoryGroup workspaceName={workspaceName} histories={histories} />
        </>
      ) : undefined}
    </>
  );
};

export const FootprintPage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { workspaceName, digest } = (rawQuery as unknown) as Query;
  const pageTitle = `Footprint: ${digest.slice(0, 8)}`;
  return (
    <div className="container">
      <Head>
        <title>
          {pageTitle} - {applicationName}
        </title>
      </Head>
      <GlobalNav workspaceName={workspaceName} />
      <h1>{pageTitle}</h1>
      {props.response != null ? <ResponseView response={props.response} /> : <p>Some error occured: {props.err}</p>}
    </div>
  );
};

FootprintPage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const { workspaceName, digest } = (rawQuery as unknown) as Query;
    const path = uria`${workspaceName}/footprints/${digest}`;
    const { data } = await defaultInstance.get(path);
    return { response: data };
  } catch (err) {
    // console.error(err);
    return { err: err.message };
  }
};

export default FootprintPage;
