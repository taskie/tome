import { IchStat } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";

type Props = {
  stat: IchStat;
};

export const StatGroupItem: React.FC<Props> = ({
  stat: { group_id, path, version, mtime, size, digest, updated_at },
}) => {
  return (
    <li>
      {digest != null ? <FootprintLink digest={digest} length={8} /> : undefined}
      {" / "}
      {mtime != null ? mtime : "Nothing"}
      {" / "}
      {updated_at}
      {" / "}
      <GroupLink groupId={group_id} />
      {" / "}
      <StatLink groupId={group_id} path={path} />
      {" / "}
      {version}
      {" / "}
      {size != null ? size : "Nothing"}
    </li>
  );
};

export default StatGroupItem;
