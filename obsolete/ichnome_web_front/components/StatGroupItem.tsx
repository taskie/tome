import { IchStat } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";
import DiffLink from "./DiffLink";

type Props = {
  workspaceName: string;
  groupName?: string;
  stat: IchStat;
  mode?: string;
  diffSource?: { groupName: string; pathPrefix: string };
};

export const StatGroupItem: React.FC<Props> = ({
  workspaceName,
  groupName,
  stat: { path, group_name, version, mtime, size, digest, updated_at },
  mode,
  diffSource,
}) => {
  const finalGroupName = group_name ?? groupName ?? "default";
  const diffMode = mode === "diff";
  if (diffMode) {
    return (
      <li>
        {mtime != null ? mtime : "Nothing"}
        {" / "}
        <GroupLink workspaceName={workspaceName} groupName={finalGroupName} />
        {" / "}
        <StatLink workspaceName={workspaceName} groupName={finalGroupName} path={path} />
        {" / "}
        {size != null ? size : "Nothing"}
      </li>
    );
  }
  return (
    <li>
      {digest != null ? <FootprintLink workspaceName={workspaceName} digest={digest} length={8} /> : undefined}
      {" / "}
      {mtime != null ? mtime : "Nothing"}
      {" / "}
      {updated_at}
      {" / "}
      <GroupLink workspaceName={workspaceName} groupName={finalGroupName} />
      {" / "}
      <StatLink workspaceName={workspaceName} groupName={finalGroupName} path={path} />
      {" / "}
      {version}
      {" / "}
      {size != null ? size : "Nothing"}
      {diffSource != null && (diffSource.groupName !== finalGroupName || diffSource.pathPrefix !== path) ? (
        <>
          {" / "}
          <DiffLink
            workspaceName={workspaceName}
            groupName1={diffSource.groupName}
            pathPrefix1={diffSource.pathPrefix}
            groupName2={finalGroupName}
            pathPrefix2={path}
          />
        </>
      ) : undefined}
    </li>
  );
};

export default StatGroupItem;
