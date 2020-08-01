import { NextPage } from "next";
import Head from "next/head";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";
import { defaultInstance } from "@/api/apiClient";
import { applicationName } from "@/config";
import { GetStatsResponse } from "@/api/types";
import Group from "@/components/Group";
import StatGroup from "@/components/StatGroup";
import { useForm } from "react-hook-form";
import Link from "next/link";

type Query = {
  workspaceName: string;
  groupName: string;
  path_prefix?: string;
  updated_at_after?: string;
  updated_at_before?: string;
};

type Response = GetStatsResponse;

type Props = { response?: Response; err?: string };

const PagenationView: React.FC<{ path_prefix?: string; updated_at_before: string }> = ({
  path_prefix,
  updated_at_before,
}) => {
  return (
    <>
      <div>
        <Link href={toQuery(rejectEmpty({ path_prefix, updated_at_before }))}>
          <a>&raquo; Show Older (Updated At Before: {updated_at_before})</a>
        </Link>
      </div>
    </>
  );
};

const ResponseView: React.FC<{
  response: Response;
  workspaceName: string;
  groupName: string;
  path_prefix?: string;
}> = ({ response: { group, stats }, workspaceName, groupName, path_prefix }) => {
  return (
    <>
      <h2>Stats</h2>
      {stats.length > 0 ? <PagenationView updated_at_before={stats[stats.length - 1].updated_at} /> : undefined}
      <StatGroup workspaceName={workspaceName} groupName={groupName} stats={stats} />
      {stats.length > 0 ? <PagenationView updated_at_before={stats[stats.length - 1].updated_at} /> : undefined}
      <h2>Group</h2>
      <Group workspaceName={workspaceName} group={group} />
    </>
  );
};

type StatsFormProps = {
  initialFormData: FormData;
  onSubmit: (form: FormData) => void;
};

type FormData = {
  path_prefix?: string;
  updated_at_after?: string;
  updated_at_before?: string;
};

function rejectEmpty<K extends string, V extends string | null | undefined, M extends Record<K, V>>(m: M): Partial<M> {
  const result: Partial<M> = {};
  for (const [k, v] of Object.entries(m) as [K, V][]) {
    if (v != null && v.length !== 0) {
      (result[k] as V) = v;
    }
  }
  return result;
}

function toQuery(m: Record<string, string | string[] | null | undefined>): string {
  const kvs = [];
  for (const [k, v] of Object.entries(m)) {
    if (v != null) {
      kvs.push(uria`${k}=${v}`);
    }
  }
  if (kvs.length > 0) {
    return `?${kvs.join("&")}`;
  } else {
    return "";
  }
}

export const StatsForm: React.FC<StatsFormProps> = ({ onSubmit, initialFormData }) => {
  const { register, handleSubmit } = useForm<FormData>({ defaultValues: initialFormData });
  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <dl>
        <dt>
          <label>Path Prefix:</label>
        </dt>
        <dd>
          <input type="text" name="path_prefix" placeholder="/data" ref={register} />
        </dd>
        <dt>
          <label>Updated At:</label>
        </dt>
        <dd>
          <input type="text" name="updated_at_after" placeholder="YYYY-mm-ddTHH:MM:SSZ" ref={register} />
          {" - "}
          <input type="text" name="updated_at_before" placeholder="YYYY-mm-ddTHH:MM:SSZ" ref={register} />
        </dd>
      </dl>
      <button>Show</button>
    </form>
  );
};

export const StatsPage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { workspaceName, groupName, path_prefix, updated_at_after, updated_at_before } = (rawQuery as unknown) as Query;
  const changeUrl = ({ path_prefix, updated_at_after, updated_at_before }: FormData) => {
    router.push(uria`/${workspaceName}/stats/${groupName}`, {
      query: rejectEmpty({
        path_prefix,
        updated_at_after,
        updated_at_before,
      }),
    });
  };
  return (
    <div className="container">
      <Head>
        <title>
          Stats of {groupName} - {applicationName}
        </title>
      </Head>
      <h1>Stats of {groupName}</h1>
      <StatsForm initialFormData={{ path_prefix, updated_at_after, updated_at_before }} onSubmit={changeUrl} />
      {props.response != null ? (
        <ResponseView response={props.response} workspaceName={workspaceName} groupName={groupName} />
      ) : (
        <p>Some error occured: {props.err}</p>
      )}
    </div>
  );
};

StatsPage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const {
      workspaceName,
      groupName,
      path_prefix,
      updated_at_after,
      updated_at_before,
    } = (rawQuery as unknown) as Query;
    const path = uria`${workspaceName}/stats/${groupName}`;
    const { data } = await defaultInstance.get(path, { params: { path_prefix, updated_at_after, updated_at_before } });
    return { response: data };
  } catch (err) {
    console.error(err);
    return { err: err.message };
  }
};

export default StatsPage;
