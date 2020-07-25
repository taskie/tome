import { NextPage } from "next";
import Head from "next/head";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";
import { defaultInstance } from "@/api/apiClient";
import { applicationName } from "@/config";
import { GetStatsResponse } from "@/api/types";
import Group from "@/components/Group";
import Stat from "@/components/Stat";
import StatGroup from "@/components/StatGroup";

type Query = {
  groupId: string;
};

type Response = GetStatsResponse;

type Props = { response?: Response; err?: string };

const ResponseView: React.FC<{ response: Response }> = ({ response: { group, stats } }) => {
  return (
    <>
      <h2>Stats</h2>
      <StatGroup stats={stats} />
      <h2>Group</h2>
      <Group group={group} />
    </>
  );
};

export const StatsPage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { groupId } = (rawQuery as unknown) as Query;
  return (
    <div className="container">
      <Head>
        <title>
          Stats of {groupId} - {applicationName}
        </title>
      </Head>
      <h1>Stats of {groupId}</h1>
      {props.response != null ? <ResponseView response={props.response} /> : <p>Some error occured: {props.err}</p>}
    </div>
  );
};

StatsPage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const { groupId } = (rawQuery as unknown) as Query;
    const path = uria`stats/${groupId}`;
    const { data } = await defaultInstance.get(path);
    return { response: data };
  } catch (err) {
    console.error(err);
    return { err: err.message };
  }
};

export default StatsPage;
