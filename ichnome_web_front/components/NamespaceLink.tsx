import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  groupId: string;
  family?: string;
};

export const GroupLink: React.FC<Props> = ({ groupId, family }) => {
  const href = family === "stats" ? uri`/stats/${groupId}` : uri`/groups/${groupId}`;
  return (
    <Link href={href}>
      <a>{groupId}</a>
    </Link>
  );
};

export default GroupLink;
